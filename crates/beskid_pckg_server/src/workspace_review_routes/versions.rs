use super::contracts::VersionBump;

pub(super) fn next_version<'a>(versions: impl Iterator<Item = &'a str>, bump: VersionBump) -> String {
    versions
        .filter_map(parse_version)
        .max()
        .map(|(major, minor, patch)| match bump {
            VersionBump::Patch => format!("{major}.{minor}.{}", patch + 1),
            VersionBump::Minor => format!("{major}.{}.0", minor + 1),
            VersionBump::Major => format!("{}.0.0", major + 1),
        })
        .unwrap_or_else(|| "0.0.1".to_owned())
}

fn parse_version(value: &str) -> Option<(u64, u64, u64)> {
    let mut parts = value.split('.');
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}
