//! Corelib API-shape tier resolver: parse `@tier(...)` directives and stamp
//! a normalized tier string onto every [`ApiDocItem`] row before `api.json`
//! is serialized.
//!
//! Spec: `site/website/src/content/docs/platform-spec/core-library/stability-and-api-shape/corelib-api-shape/`
//!
//! Directive grammar is intentionally permissive — the resolver accepts the
//! lowercase canonical values (`standard`, `supported`, `unstable`) as well
//! as the `Tier1` / `Tier2` / `Tier3` and capitalized aliases that the
//! `Corelib API shape` Feature Hub Design model documents. Unknown values are
//! ignored (left as `None`) so unrecognized tiers cannot leak into `api.json`.

use std::collections::HashMap;

use crate::doc::api_snapshot::ApiDocItem;

/// Canonical lowercase tier values that may appear on `ApiDocItem.tier`.
pub const TIER_STANDARD: &str = "standard";
/// Default tier for public corelib items without an explicit directive.
pub const TIER_SUPPORTED: &str = "supported";
/// Tier-3 unstable surfaces — no compatibility guarantee.
pub const TIER_UNSTABLE: &str = "unstable";

/// Parse a single `@tier(<value>)` directive from a raw doc body and return
/// the normalized canonical tier string.
///
/// Returns `None` when no `@tier(...)` directive is present or when the value
/// is not recognized by the closed alias set.
pub fn parse_tier_directive(body: &str) -> Option<String> {
    let needle = "@tier(";
    let mut search_from = 0;
    let mut last_match: Option<String> = None;
    while let Some(rel) = body[search_from..].find(needle) {
        let open = search_from + rel + needle.len();
        let Some(close_rel) = body[open..].find(')') else {
            break;
        };
        let value = body[open..open + close_rel].trim();
        if let Some(canonical) = canonicalize_tier_value(value) {
            last_match = Some(canonical);
        }
        search_from = open + close_rel + 1;
    }
    last_match
}

fn canonicalize_tier_value(raw: &str) -> Option<String> {
    let lower = raw.to_ascii_lowercase();
    match lower.as_str() {
        "standard" | "tier1" | "tier-1" | "tier 1" => Some(TIER_STANDARD.to_string()),
        "supported" | "tier2" | "tier-2" | "tier 2" => Some(TIER_SUPPORTED.to_string()),
        "unstable" | "tier3" | "tier-3" | "tier 3" => Some(TIER_UNSTABLE.to_string()),
        _ => None,
    }
}

/// Resolve the tier for every row in `items` and stamp the result into
/// `ApiDocItem.tier`. The resolver applies the parent-default cascade
/// documented on the Feature Hub: item-site directive > parent row tier >
/// workspace default (which is left as `None` here; consumers treat the
/// absence as `supported`).
pub fn resolve_item_tiers(items: &mut [ApiDocItem]) {
    let direct: Vec<Option<String>> = items
        .iter()
        .map(|item| {
            item.doc_markdown
                .as_deref()
                .and_then(parse_tier_directive)
                .or_else(|| {
                    item.doc
                        .as_ref()
                        .and_then(|d| d.summary_markdown.as_deref())
                        .and_then(parse_tier_directive)
                })
        })
        .collect();

    let by_id: HashMap<usize, usize> = items
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| item.id.map(|id| (id, idx)))
        .collect();

    let mut resolved: Vec<Option<String>> = direct.clone();
    for idx in 0..items.len() {
        if resolved[idx].is_some() {
            continue;
        }
        let mut cursor = items[idx].parent_id;
        let mut guard = 0_usize;
        while let Some(pid) = cursor {
            guard += 1;
            if guard > 1024 {
                break;
            }
            let Some(&parent_idx) = by_id.get(&pid) else {
                break;
            };
            if let Some(parent_tier) = resolved[parent_idx].clone() {
                resolved[idx] = Some(parent_tier);
                break;
            }
            cursor = items[parent_idx].parent_id;
        }
    }

    for (item, tier) in items.iter_mut().zip(resolved.into_iter()) {
        item.tier = tier;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: usize, parent: Option<usize>, doc: &str) -> ApiDocItem {
        ApiDocItem {
            id: Some(id),
            qualified_name: format!("item{id}"),
            name: format!("item{id}"),
            kind: "function".to_string(),
            visibility: Some("public".to_string()),
            location: crate::doc::api_snapshot::ApiLocation {
                file: "x.bd".to_string(),
                start_line: 1,
                start_column: 1,
                end_line: 1,
                end_column: 1,
            },
            parent_id: parent,
            member_ids: Vec::new(),
            display_name: None,
            module_path: Vec::new(),
            signature: None,
            field_type: None,
            return_type: None,
            parameters: Vec::new(),
            generic_parameters: Vec::new(),
            doc_markdown: if doc.is_empty() {
                None
            } else {
                Some(doc.to_string())
            },
            doc: None,
            declaring_package: None,
            controls: vec![],
            tier: None,
        }
    }

    #[test]
    fn resolves_each_directive_spelling() {
        assert_eq!(
            parse_tier_directive("/// @tier(standard)"),
            Some(TIER_STANDARD.to_string())
        );
        assert_eq!(
            parse_tier_directive("/// @tier(Tier1)"),
            Some(TIER_STANDARD.to_string())
        );
        assert_eq!(
            parse_tier_directive("/// @tier(SUPPORTED) summary"),
            Some(TIER_SUPPORTED.to_string())
        );
        assert_eq!(
            parse_tier_directive("/// notes\n/// @tier(unstable)\n"),
            Some(TIER_UNSTABLE.to_string())
        );
        assert_eq!(
            parse_tier_directive("/// @tier( Tier3 )"),
            Some(TIER_UNSTABLE.to_string())
        );
    }

    #[test]
    fn missing_directive_yields_none() {
        assert_eq!(parse_tier_directive("/// summary only"), None);
        assert_eq!(parse_tier_directive(""), None);
    }

    #[test]
    fn unknown_directive_value_is_ignored() {
        assert_eq!(parse_tier_directive("/// @tier(legacy)"), None);
        assert_eq!(parse_tier_directive("/// @tier(experimental)"), None);
    }

    #[test]
    fn missing_directive_defaults_to_supported_via_consumer() {
        // The resolver itself leaves the row as `None`; this asserts the
        // contract documented on the Feature Hub: consumers treat the
        // absence as `supported`.
        let mut items = vec![item(1, None, "/// summary")];
        resolve_item_tiers(&mut items);
        assert_eq!(items[0].tier, None);
    }

    #[test]
    fn item_site_directive_wins_over_parent() {
        let mut items = vec![
            item(1, None, "/// @tier(unstable)"),
            item(2, Some(1), "/// @tier(standard)"),
        ];
        resolve_item_tiers(&mut items);
        assert_eq!(items[1].tier.as_deref(), Some(TIER_STANDARD));
    }

    #[test]
    fn member_inherits_parent_tier_when_missing() {
        let mut items = vec![
            item(1, None, "/// @tier(supported)"),
            item(2, Some(1), "/// member without tier"),
        ];
        resolve_item_tiers(&mut items);
        assert_eq!(items[0].tier.as_deref(), Some(TIER_SUPPORTED));
        assert_eq!(items[1].tier.as_deref(), Some(TIER_SUPPORTED));
    }

    #[test]
    fn cascade_walks_multiple_levels() {
        let mut items = vec![
            item(1, None, "/// @tier(standard)"),
            item(2, Some(1), "/// no tier"),
            item(3, Some(2), "/// no tier either"),
        ];
        resolve_item_tiers(&mut items);
        assert_eq!(items[2].tier.as_deref(), Some(TIER_STANDARD));
    }

    #[test]
    fn cascade_stops_at_unknown_parent_without_panic() {
        let mut items = vec![item(2, Some(999), "/// no tier")];
        resolve_item_tiers(&mut items);
        assert_eq!(items[0].tier, None);
    }

    #[test]
    fn last_directive_wins_when_multiple_present() {
        let body = "/// @tier(supported)\n/// later thoughts\n/// @tier(standard)";
        assert_eq!(parse_tier_directive(body), Some(TIER_STANDARD.to_string()));
    }

    #[test]
    fn tier_field_round_trips_through_serde() {
        let mut row = item(1, None, "/// @tier(standard)");
        resolve_item_tiers(std::slice::from_mut(&mut row));
        assert_eq!(row.tier.as_deref(), Some(TIER_STANDARD));
        let json = serde_json::to_string(&row).expect("serialize");
        assert!(
            json.contains("\"tier\":\"standard\""),
            "tier should serialize as camelCase lowercase: {json}"
        );
        let de: ApiDocItem = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(de.tier.as_deref(), Some(TIER_STANDARD));
    }
}
