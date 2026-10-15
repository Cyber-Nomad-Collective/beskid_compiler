//! Attribute target legality (E1508, E1509, E1510), judged once per unit.
//!
//! Every attribute declaration lists its targets from the closed catalogue
//! (`beskid_analysis::attribute_targets`): a name outside it is E1509 at the target, a name
//! listed twice is E1508. Every attribute use is resolved to a declaration and checked against
//! its target list (E1510); a use that resolves to nothing is E1510 with `declared: false`.
//!
//! Resolution of a use named `N` in unit `U`, first match wins:
//! 1. a declaration of `N` in `U` (top level or inline module);
//! 2. a declaration of `N` in a unit `U` imports (`use` and module edges);
//! 3. a compiler-recognized `N`: its corelib declaration in `Core.Attributes` when that unit is
//!    assembled, otherwise the compiler's copy of the same target list;
//! 4. any other declaration of `N` in the assembly (Mod-declared attributes reach the program as
//!    merged items of the entry unit). Several such declarations allow the union of their
//!    targets.
//!
//! A recognized name that resolves at step 1 or 2 to a declaration outside `Core.Attributes` is
//! an ordinary user attribute (shadowing), and [`resolve_attribute_use`] reports it as not
//! recognized.

use super::*;
use beskid_analysis::attribute_targets::{
    RECOGNIZED_ATTRIBUTE_MODULE, attribute_site_target, attribute_target_catalogue, is_attribute_target,
    recognized_attribute,
};
use beskid_analysis::syntax::{Attribute, AttributeDeclaration};

/// One attribute declaration of a unit.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct UnitAttributeDeclaration {
    pub node: beskid_analysis::syntax::AstNodeId,
    pub name: Arc<str>,
    pub targets: Arc<[Arc<str>]>,
}

/// One attribute use of a unit with the catalogue name of the site that carries it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct UnitAttributeUse {
    pub node: beskid_analysis::syntax::AstNodeId,
    pub name: Arc<str>,
    pub site: Arc<str>,
}

/// The attribute surface of one unit: declarations, target nodes per declaration, and uses.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct UnitAttributeSurface {
    pub declarations: Arc<[UnitAttributeDeclaration]>,
    /// Per declaration (same order): the `AttributeTarget` node of each listed target.
    pub target_nodes: Arc<[Arc<[beskid_analysis::syntax::AstNodeId]>]>,
    pub uses: Arc<[UnitAttributeUse]>,
}

/// The attribute surface of the unit rooted at `key`.
pub fn unit_attribute_surface(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<UnitAttributeSurface> {
    with_registered_syntax(db, key, unit_attribute_surface_tracked)
}

#[salsa::tracked(persist)]
fn unit_attribute_surface_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<UnitAttributeSurface> {
    with_node(db, syntax, key, |program, index, _node| {
        if index.kind(key.node) != Some(NodeKind::Program) {
            return None;
        }
        let mut declarations = Vec::new();
        let mut target_nodes = Vec::new();
        for node in index.ids_of_kind(NodeKind::AttributeDeclaration) {
            let Some(declaration) = index.node_at(program, node).and_then(|node| node.of::<AttributeDeclaration>())
            else {
                continue;
            };
            let targets = index
                .children(node)
                .unwrap_or_default()
                .iter()
                .copied()
                .filter(|child| index.kind(*child) == Some(NodeKind::AttributeTarget))
                .collect::<Vec<_>>();
            declarations.push(UnitAttributeDeclaration {
                node,
                name: Arc::from(declaration.name.node.name.as_str()),
                targets: declaration
                    .targets
                    .iter()
                    .map(|target| Arc::from(target.node.name.node.name.as_str()))
                    .collect::<Vec<_>>()
                    .into(),
            });
            target_nodes.push(Arc::from(targets));
        }
        let uses = index
            .ids_of_kind(NodeKind::Attribute)
            .filter_map(|node| {
                let parent = parent_node(index, node)?;
                let site = attribute_site_target(index.kind(parent)?)?;
                let attribute = index.node_at(program, node)?.of::<Attribute>()?;
                Some(UnitAttributeUse { node, name: Arc::from(attribute.name.node.name.as_str()), site: Arc::from(site) })
            })
            .collect::<Vec<_>>();
        Some(UnitAttributeSurface { declarations: declarations.into(), target_nodes: target_nodes.into(), uses: uses.into() })
    })
}

/// What an attribute use names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttributeResolution {
    /// A source declaration (or several at the assembly-wide step, whose targets are united).
    Declared {
        declaration: AstNodeKey,
        targets: Vec<Arc<str>>,
        /// The declaration is the corelib declaration of a compiler-recognized attribute.
        recognized: bool,
    },
    /// A compiler-recognized attribute whose corelib declaration is not assembled.
    Recognized { targets: &'static [&'static str] },
    /// Nothing of that name is in scope.
    Undeclared,
}

impl AttributeResolution {
    /// Whether the use names a compiler-recognized attribute (not a user declaration that
    /// shadows one).
    pub fn is_recognized(&self) -> bool {
        matches!(self, Self::Declared { recognized: true, .. } | Self::Recognized { .. })
    }

    fn allows(&self, site: &str) -> Option<bool> {
        match self {
            Self::Declared { targets, .. } => Some(targets.iter().any(|target| target.as_ref() == site)),
            Self::Recognized { targets } => Some(targets.contains(&site)),
            Self::Undeclared => None,
        }
    }

    fn allowed(&self) -> Vec<String> {
        match self {
            Self::Declared { targets, .. } => targets.iter().map(|target| target.to_string()).collect(),
            Self::Recognized { targets } => targets.iter().map(|target| (*target).to_string()).collect(),
            Self::Undeclared => Vec::new(),
        }
    }
}

fn unit_root(unit: SourceUnitId, generation: SyntaxGenerationId) -> AstNodeKey {
    AstNodeKey { unit, generation, node: beskid_analysis::syntax::AstNodeId(0) }
}

fn declarations_named(db: &dyn Db, root: AstNodeKey, name: &str) -> Vec<(AstNodeKey, Arc<[Arc<str>]>)> {
    let Ok(Some(surface)) = unit_attribute_surface(db, root) else { return Vec::new() };
    surface
        .declarations
        .iter()
        .filter(|declaration| declaration.name.as_ref() == name)
        .map(|declaration| (AstNodeKey { node: declaration.node, ..root }, declaration.targets.clone()))
        .collect()
}

fn recognized_module_units(db: &dyn Db, generation: SyntaxGenerationId) -> Vec<SourceUnitId> {
    let path = RECOGNIZED_ATTRIBUTE_MODULE.iter().map(|segment| (*segment).to_string()).collect::<Vec<_>>();
    let registry = db.syntax_dependency_registry().lock().expect("syntax dependency registry");
    registry.visible_module_units(generation, &path).map(<[_]>::to_vec).unwrap_or_default()
}

fn declared(found: Vec<(AstNodeKey, Arc<[Arc<str>]>)>, recognized_units: &[SourceUnitId], name: &str) -> AttributeResolution {
    let declaration = found[0].0;
    let mut targets = Vec::<Arc<str>>::new();
    for (_, listed) in &found {
        for target in listed.iter() {
            if !targets.contains(target) {
                targets.push(target.clone());
            }
        }
    }
    let recognized = recognized_attribute(name).is_some()
        && found.iter().all(|(key, _)| recognized_units.contains(&key.unit));
    AttributeResolution::Declared { declaration, targets, recognized }
}

/// Resolve the attribute name `name` used in the unit rooted at `root`.
pub fn resolve_attribute_use(db: &dyn Db, root: AstNodeKey, name: &str) -> AttributeResolution {
    let generation = root.generation;
    let recognized_units = recognized_module_units(db, generation);
    let own = declarations_named(db, root, name);
    if !own.is_empty() {
        return declared(own, &recognized_units, name);
    }
    let (imported, all_units) = {
        let registry = db.syntax_dependency_registry().lock().expect("syntax dependency registry");
        let imported = registry
            .imports
            .get(&(root.unit, generation))
            .map(|imports| imports.iter().map(|import| import.target).collect::<Vec<_>>())
            .unwrap_or_default();
        let all_units = registry
            .modules
            .iter()
            .filter(|((module_generation, _), _)| *module_generation == generation)
            .flat_map(|(_, units)| units.iter().copied())
            .collect::<Vec<_>>();
        (imported, all_units)
    };
    let mut all_units = all_units.into_iter().fold(Vec::new(), |mut units, unit| {
        if !units.contains(&unit) {
            units.push(unit);
        }
        units
    });
    all_units.sort_by(|left, right| left.path(db).cmp(right.path(db)));
    let mut seen = vec![root.unit];
    let mut found = Vec::new();
    for unit in imported {
        if seen.contains(&unit) {
            continue;
        }
        seen.push(unit);
        found.extend(declarations_named(db, unit_root(unit, generation), name));
    }
    if !found.is_empty() {
        return declared(found, &recognized_units, name);
    }
    if let Some(attribute) = recognized_attribute(name) {
        let corelib = recognized_units
            .iter()
            .flat_map(|unit| declarations_named(db, unit_root(*unit, generation), name))
            .collect::<Vec<_>>();
        if !corelib.is_empty() {
            return declared(corelib, &recognized_units, name);
        }
        return AttributeResolution::Recognized { targets: attribute.targets };
    }
    let found = all_units
        .into_iter()
        .filter(|unit| !seen.contains(unit))
        .flat_map(|unit| declarations_named(db, unit_root(unit, generation), name))
        .collect::<Vec<_>>();
    if found.is_empty() { AttributeResolution::Undeclared } else { declared(found, &recognized_units, name) }
}

/// The E1508, E1509 and E1510 findings of the unit rooted at `root`, in node order.
pub fn attribute_findings(db: &dyn Db, root: AstNodeKey) -> Vec<SemanticFinding> {
    let Ok(Some(surface)) = unit_attribute_surface(db, root) else { return Vec::new() };
    let mut findings = Vec::new();
    for (declaration, nodes) in surface.declarations.iter().zip(surface.target_nodes.iter()) {
        let mut seen: Vec<(&str, beskid_analysis::syntax::AstNodeId)> = Vec::new();
        for (target, node) in declaration.targets.iter().zip(nodes.iter()) {
            let site = AstNodeKey { node: *node, ..root };
            if !is_attribute_target(target) {
                findings.push(SemanticFinding {
                    kind: SemanticIssueKind::UnknownAttributeDeclarationTarget {
                        target: target.to_string(),
                        allowed: attribute_target_catalogue(),
                    },
                    site,
                    related: Vec::new(),
                });
            } else if let Some((_, previous)) = seen.iter().find(|(name, _)| *name == target.as_ref()) {
                let previous = node_span(db, AstNodeKey { node: *previous, ..root }).ok().flatten().unwrap_or_default();
                findings.push(SemanticFinding {
                    kind: SemanticIssueKind::DuplicateAttributeDeclarationTarget { target: target.to_string(), previous },
                    site,
                    related: Vec::new(),
                });
            } else {
                seen.push((target.as_ref(), *node));
            }
        }
    }
    for attribute_use in surface.uses.iter() {
        let resolution = resolve_attribute_use(db, root, &attribute_use.name);
        if resolution.allows(&attribute_use.site) == Some(true) {
            continue;
        }
        findings.push(SemanticFinding {
            kind: SemanticIssueKind::AttributeTargetNotAllowed {
                attribute: attribute_use.name.to_string(),
                target: attribute_use.site.to_string(),
                allowed: resolution.allowed(),
                declared: !matches!(resolution, AttributeResolution::Undeclared),
            },
            site: AstNodeKey { node: attribute_use.node, ..root },
            related: Vec::new(),
        });
    }
    findings.sort_by_key(|finding| finding.site.node);
    findings
}

/// The attribute findings of every unit in `roots` (deduplicated by unit).
pub fn check_attribute_obligations(db: &dyn Db, roots: &[AstNodeKey]) -> Vec<SemanticFinding> {
    let mut judged = Vec::new();
    let mut findings = Vec::new();
    for root in roots {
        let root = AstNodeKey { node: beskid_analysis::syntax::AstNodeId(0), ..*root };
        if judged.contains(&root) {
            continue;
        }
        judged.push(root);
        findings.extend(attribute_findings(db, root));
    }
    findings
}
