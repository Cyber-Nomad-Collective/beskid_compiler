use super::{
    Attribute, Cell, Color, ContentArrangement, PackageVersionSummaryResponse, PckgError, Table, UTF8_FULL,
    UTF8_ROUND_CORNERS,
};

pub(super) fn print_package_versions_table(versions: &[PackageVersionSummaryResponse]) {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("Package").add_attribute(Attribute::Bold),
            Cell::new("Version").add_attribute(Attribute::Bold),
            Cell::new("Yanked").add_attribute(Attribute::Bold),
            Cell::new("Checksum").add_attribute(Attribute::Bold),
            Cell::new("Size").add_attribute(Attribute::Bold),
            Cell::new("Published").add_attribute(Attribute::Bold),
        ]);

    for version in versions {
        table.add_row(vec![
            Cell::new(&version.package_name),
            Cell::new(&version.version).fg(Color::Cyan),
            Cell::new(if version.is_yanked { "yes" } else { "no" }),
            Cell::new(&version.checksum_sha256),
            Cell::new(version.size_bytes.to_string()),
            Cell::new(&version.published_at_utc),
        ]);
    }

    println!("{table}");
}
pub(super) fn print_pckg_error_human(err: &PckgError) {
    eprintln!("pckg error: {err}");
    match err {
        PckgError::Api { body: Some(b), .. } | PckgError::LogicalFailure { body: Some(b), .. } => {
            let snippet: String = b.chars().take(2000).collect();
            if !snippet.is_empty() {
                eprintln!("response body (truncated): {snippet}");
            }
        }
        _ => {}
    }
}
