use beskid_analysis::doc::{ApiDocLinkContext, DocRefLinkContext, build_api_doc_link_context};
use beskid_analysis::projects::load_manifest_from_path;

pub(super) fn api_doc_link_context(resolved: &beskid_analysis::services::ResolvedInput) -> Option<ApiDocLinkContext> {
    let plan = resolved.compile_plan.as_ref()?;
    build_api_doc_link_context(plan, resolved.prepared_workspace.as_ref())
}

pub(super) fn docs_ref_link_context(resolved: &beskid_analysis::services::ResolvedInput) -> Option<DocRefLinkContext> {
    let plan = resolved.compile_plan.as_ref()?;
    let manifest = load_manifest_from_path(&plan.manifest_path).ok()?;
    let name = manifest.project.name.trim();
    let ver = manifest.project.version.trim();
    if name.is_empty() || ver.is_empty() {
        return None;
    }
    let mut ctx = DocRefLinkContext {
        package_with_version: format!("{name}@{ver}"),
        publishing_package: Some(name.to_string()),
        dependency_roots: vec![],
    };
    if let Some(link_ctx) = api_doc_link_context(resolved) {
        ctx.publishing_package = Some(link_ctx.publishing_package.clone());
        ctx.dependency_roots = link_ctx
            .packages
            .iter()
            .filter(|pkg| pkg.package != link_ctx.publishing_package)
            .map(|pkg| (pkg.match_root.clone(), pkg.package.clone()))
            .collect();
    }
    Some(ctx)
}
