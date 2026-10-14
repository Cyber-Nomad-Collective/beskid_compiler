//! One target-free dependency resolution authority for preparation and mutations.
#![allow(non_snake_case)]

use super::lockfile::{
    preflight_existing_lock_for_plan, reject_unreachable_registry_pins, validate_existing_lock_graph,
};
use super::prepare::{installed_corelib_lock_root, portable_entry_for_dependency};
use super::registry::{ResolvedRegistryDependency, parse_registry_descriptor, resolve_registry_dependency};
use super::{ProjectLockDependencyEntry, ProjectLockSource, WorkspacePrepareOptions};
use crate::projects::{
    DependencySource, ProjectError, ProjectWorkspacePlan, ResolvedDependencyProject, UnresolvedDependencyNote,
    discover_workspace_resolution_rules, is_core_dependency_name,
};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum RefreshScope {
    #[default]
    None,
    Selected(BTreeSet<String>),
    All,
}
impl RefreshScope {
    pub(crate) fn Includes(&self, name: &str) -> bool {
        match self {
            Self::None => false,
            Self::Selected(names) => names.contains(name),
            Self::All => true,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolutionPolicy {
    pub locked: bool,
    pub offline: bool,
    pub refresh: RefreshScope,
}

pub(super) struct ResolvedWorkspaceDependencies {
    pub entries: Vec<ProjectLockDependencyEntry>,
    pub registry: Vec<ResolvedRegistryDependency>,
}

pub(super) fn ResolveWorkspaceDependencies(
    plan: &ProjectWorkspacePlan,
    policy: &ResolutionPolicy,
    allow_graph_change: bool,
) -> Result<ResolvedWorkspaceDependencies, ProjectError> {
    if policy.locked && (allow_graph_change || policy.refresh != RefreshScope::None) {
        return Err(ProjectError::Validation("locked resolution forbids dependency mutation".into()));
    }
    let options = WorkspacePrepareOptions {
        offline: policy.offline,
        locked: policy.locked,
        frozen: false,
        refresh_lock: allow_graph_change || policy.refresh == RefreshScope::All,
    };
    let existing = preflight_existing_lock_for_plan(plan, options)?;
    let corelib = installed_corelib_lock_root();
    RejectAmbiguousDependencyNames(&plan.dependency_projects, corelib.as_deref())?;
    let mut entries = Vec::new();
    let mut destinations = HashSet::new();
    for dependency in &plan.dependency_projects {
        let entry = portable_entry_for_dependency(plan, dependency, corelib.as_deref())?;
        if !destinations.insert(entry.materialized_root.clone()) {
            return Err(ProjectError::Validation("lockfile duplicates a materialized destination".into()));
        }
        entries.push(entry);
    }
    validate_existing_lock_graph(existing.as_ref(), &entries, options.refresh_lock)?;
    let rules = discover_workspace_resolution_rules(&plan.manifest_path)?;
    // A mutation (graph change or refresh) may select coordinates for transitive packages whose
    // requirement changed; ordinary, locked and offline resolution only replays pins.
    let mutable = allow_graph_change || policy.refresh != RefreshScope::None;
    let roots = plan
        .unresolved_dependencies
        .iter()
        .filter(|dependency| dependency.source == DependencySource::Registry)
        .map(|note| RegistryRequest::New(note.clone(), true))
        .collect::<Vec<_>>();
    let closure = ResolveRegistryClosure(roots, |request| {
        let note = &request.note;
        let pin = existing.as_ref().and_then(|lock| {
            lock.dependencies
                .iter()
                .find(|entry| entry.source == ProjectLockSource::Registry && entry.name == note.dependency_name)
        });
        let stale = pin.is_some_and(|pin| request.PinIsStale(pin));
        let refresh = policy.refresh.Includes(&note.dependency_name)
            || (mutable && (stale || (!request.direct && pin.is_none())));
        let unpinned_in_existing_lock = existing.is_some() && pin.is_none() && !allow_graph_change && !refresh;
        let recovery = if request.direct {
            format!("beskid update {}", note.dependency_name)
        } else {
            "beskid update --all".to_owned()
        };
        if unpinned_in_existing_lock && (policy.locked || policy.offline) {
            return Err(ProjectError::Validation(format!(
                "registry dependency `{}` (required by `{}`) has no lock pin; run `{recovery}`",
                note.dependency_name, note.requester
            )));
        }
        let Some(resolved) = resolve_registry_dependency(
            note,
            rules.as_ref(),
            pin,
            refresh,
            &plan.project_root,
            policy.offline,
            request.direct && !allow_graph_change,
        )?
        else {
            return Ok(None);
        };
        if unpinned_in_existing_lock {
            return Err(ProjectError::Validation(format!(
                "registry dependency `{}` (required by `{}`) is now available but has no lock pin; run `{recovery}`",
                note.dependency_name, note.requester
            )));
        }
        resolved.validate_existing_lock_entry(if refresh { None } else { pin })?;
        let requester = format!("{}@{}", note.dependency_name, resolved.selected_version());
        let mut requires = Vec::new();
        for dependency in resolved.declared_dependencies()? {
            match dependency.source {
                DependencySource::Registry => {
                    let declared = dependency.version.clone().ok_or_else(|| {
                        ProjectError::Validation(format!(
                            "registry package `{requester}` declares registry dependency `{}` without a version",
                            dependency.name
                        ))
                    })?;
                    // Workspace overrides apply to every registry request of the graph, as they do
                    // to direct dependencies when the project graph is built.
                    let version = rules
                        .as_ref()
                        .and_then(|rules| rules.override_version_for(&dependency.name))
                        .map(str::to_owned)
                        .unwrap_or(declared);
                    let descriptor = match &dependency.registry {
                        Some(alias) => format!("{alias}@{version}"),
                        None => version,
                    };
                    requires.push(RegistryRequest::New(
                        UnresolvedDependencyNote {
                            dependency_name: dependency.name.clone(),
                            source: DependencySource::Registry,
                            descriptor,
                            requester: requester.clone(),
                        },
                        false,
                    ));
                }
                // The implicit Corelib aggregate is provided by the consuming installation.
                DependencySource::Path if is_core_dependency_name(&dependency.name) && dependency.path.is_none() => {}
                source => {
                    return Err(ProjectError::Validation(format!(
                        "registry package `{requester}` declares {} dependency `{}`; a published package may depend \
                         only on registry packages",
                        match source {
                            DependencySource::Path => "path",
                            DependencySource::Git => "git",
                            DependencySource::Registry => unreachable!(),
                        },
                        dependency.name
                    )));
                }
            }
        }
        Ok(Some((resolved, requires)))
    })?;
    if let Some(existing) = existing.as_ref()
        && !options.refresh_lock
    {
        reject_unreachable_registry_pins(existing, &closure.names.iter().map(String::as_str).collect())?;
    }
    let mut registry = Vec::new();
    for (name, resolved) in closure.resolved {
        if name == plan.project_name {
            return Err(ProjectError::Validation(format!(
                "registry package `{name}` has the name of the root project `{}`; a package cannot depend on itself",
                plan.project_name
            )));
        }
        if let Some(path) = entries.iter().find(|entry| entry.name == name) {
            return Err(ProjectError::Validation(format!(
                "dependency `{name}` resolves to two different packages: the registry package `{name}@{}` and the {} \
                 package at `{}`; rename one dependency label",
                resolved.selected_version(),
                path.source.as_str(),
                path.project
            )));
        }
        if !destinations.insert(resolved.materialized_relative.clone()) {
            return Err(ProjectError::Validation("lockfile duplicates a materialized destination".into()));
        }
        registry.push(resolved);
    }
    Ok(ResolvedWorkspaceDependencies { entries, registry })
}

/// One registry requirement edge: the requested coordinate and the package that requires it.
pub(super) struct RegistryRequest {
    note: UnresolvedDependencyNote,
    /// Registry identity (`default` or the lowercased workspace alias).
    registry: String,
    /// Requested version text (`*` selects the greatest stable version).
    version: String,
    /// Declared by the project graph (root or path dependency) rather than by a registry package.
    direct: bool,
}

impl RegistryRequest {
    fn New(note: UnresolvedDependencyNote, direct: bool) -> Self {
        let (alias, version) = parse_registry_descriptor(&note.descriptor);
        Self {
            registry: alias.as_deref().unwrap_or("default").to_ascii_lowercase(),
            version: version.unwrap_or_else(|| "*".to_owned()),
            note,
            direct,
        }
    }

    fn Name(&self) -> &str {
        &self.note.dependency_name
    }

    /// A pin no longer satisfies this request when the registry or the exact version differs.
    fn PinIsStale(&self, pin: &ProjectLockDependencyEntry) -> bool {
        pin.registry.as_deref() != Some(self.registry.as_str())
            || (self.version != "*" && pin.resolved_version.as_deref() != Some(self.version.as_str()))
    }

    /// The quoted coordinate for diagnostics, with a non-default registry named after it.
    fn Coordinate(&self) -> String {
        if self.registry == "default" {
            format!("`{}@{}`", self.Name(), self.version)
        } else {
            format!("`{}@{}` from registry `{}`", self.Name(), self.version, self.registry)
        }
    }
}

pub(super) struct RegistryClosure<T> {
    /// Every reachable registry package name, including declared packages the registry could not
    /// serve during warning-only ordinary preparation.
    pub names: BTreeSet<String>,
    /// The selected package of every resolved name, ordered by name.
    pub resolved: Vec<(String, T)>,
}

/// Resolve the registry closure: every package reachable from `roots` through registry
/// requirements, with exactly one coordinate per package name.
///
/// `resolve` selects one request's package (pin, cache or registry) and returns the requests its
/// manifest declares, or `None` when a declared direct dependency stays warning-only unresolved.
/// Requests are processed breadth-first, so every direct request is seen before any transitive one.
///
/// Conflict rule (no range solving): all requests for one package name SHALL name the same registry
/// and the same requested version text. Identical requests from different requesters (a diamond)
/// share one materialized package. Two different requests fail closed with a diagnostic naming
/// both requesters and both coordinates; a workspace `override` aligns them. One requester that
/// requests the same package twice is two declarations for one materialized destination and fails
/// closed. Requirement cycles between registry packages are rejected like path dependency cycles.
pub(super) fn ResolveRegistryClosure<T>(
    roots: Vec<RegistryRequest>,
    mut resolve: impl FnMut(&RegistryRequest) -> Result<Option<(T, Vec<RegistryRequest>)>, ProjectError>,
) -> Result<RegistryClosure<T>, ProjectError> {
    struct Node<T> {
        request: RegistryRequest,
        resolved: Option<T>,
        requires: Vec<String>,
    }
    let mut nodes: BTreeMap<String, Node<T>> = BTreeMap::new();
    let mut declarations: HashSet<(String, String)> = HashSet::new();
    let mut queue = VecDeque::from(roots);
    while let Some(request) = queue.pop_front() {
        if !declarations.insert((request.note.requester.clone(), request.Name().to_owned())) {
            return Err(ProjectError::Validation(format!(
                "`{}` declares registry package `{}` twice; two declarations cannot share one materialized \
                 destination",
                request.note.requester,
                request.Name()
            )));
        }
        if let Some(first) = nodes.get(request.Name()) {
            if first.request.registry != request.registry || first.request.version != request.version {
                return Err(ProjectError::Validation(format!(
                    "registry package `{}` is requested at two coordinates: `{}` requires {} and `{}` requires {}; \
                     one version per package is allowed, so align both requirements or add a workspace `override` \
                     for `{}`",
                    request.Name(),
                    first.request.note.requester,
                    first.request.Coordinate(),
                    request.note.requester,
                    request.Coordinate(),
                    request.Name()
                )));
            }
            if first.resolved.is_none() && !request.direct {
                return Err(ProjectError::Validation(format!(
                    "registry package {} required by `{}` is unavailable from the registry",
                    request.Coordinate(),
                    request.note.requester
                )));
            }
            continue;
        }
        let (resolved, requires) = match resolve(&request)? {
            Some((resolved, requires)) => {
                let names = requires.iter().map(|required| required.Name().to_owned()).collect::<Vec<_>>();
                queue.extend(requires);
                (Some(resolved), names)
            }
            None => (None, Vec::new()),
        };
        nodes.insert(request.Name().to_owned(), Node { request, resolved, requires });
    }
    RejectRegistryCycles(&nodes.iter().map(|(name, node)| (name.as_str(), node.requires.as_slice())).collect())?;
    Ok(RegistryClosure {
        names: nodes.keys().cloned().collect(),
        resolved: nodes.into_iter().filter_map(|(name, node)| node.resolved.map(|resolved| (name, resolved))).collect(),
    })
}

/// Reject a requirement cycle among registry packages, naming the cycle in order.
fn RejectRegistryCycles(requires: &BTreeMap<&str, &[String]>) -> Result<(), ProjectError> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Mark {
        Active,
        Done,
    }
    fn Visit<'a>(
        name: &'a str,
        requires: &BTreeMap<&'a str, &'a [String]>,
        marks: &mut HashMap<&'a str, Mark>,
        path: &mut Vec<&'a str>,
    ) -> Result<(), ProjectError> {
        match marks.get(name) {
            Some(Mark::Done) => return Ok(()),
            Some(Mark::Active) => {
                let start = path.iter().position(|item| *item == name).unwrap_or(0);
                let mut cycle = path[start..].to_vec();
                cycle.push(name);
                return Err(ProjectError::DependencyCycle(format!("registry packages {}", cycle.join(" -> "))));
            }
            None => {}
        }
        marks.insert(name, Mark::Active);
        path.push(name);
        for required in requires.get(name).copied().unwrap_or_default() {
            if let Some((required, _)) = requires.get_key_value(required.as_str()) {
                Visit(*required, requires, marks, path)?;
            }
        }
        path.pop();
        marks.insert(name, Mark::Done);
        Ok(())
    }
    let mut marks = HashMap::new();
    for name in requires.keys() {
        Visit(*name, requires, &mut marks, &mut Vec::new())?;
    }
    Ok(())
}

/// One lock entry exists per dependency name, so one name must identify one package.
///
/// The project graph already unifies every edge that reaches the same manifest into one node
/// (one package identity, as Cargo keeps one `Cargo.lock` entry per package id). Two resolved
/// dependency projects that share a name therefore live in different package roots: they are
/// different packages, and the lock cannot record both. Fail closed and name both roots.
fn RejectAmbiguousDependencyNames(
    dependencies: &[ResolvedDependencyProject],
    corelibRoot: Option<&Path>,
) -> Result<(), ProjectError> {
    let canonicalCorelib = corelibRoot.and_then(|root| root.canonicalize().ok());
    let mut seen: HashMap<&str, (&ResolvedDependencyProject, PathBuf)> = HashMap::new();
    for dependency in dependencies {
        let root = dependency.project_root.canonicalize().unwrap_or_else(|_| dependency.project_root.clone());
        let Some((first, firstRoot)) = seen.get(dependency.dependency_name.as_str()) else {
            seen.insert(dependency.dependency_name.as_str(), (dependency, root));
            continue;
        };
        if *firstRoot == root {
            continue;
        }
        let inCorelib = |path: &Path| canonicalCorelib.as_deref().is_some_and(|corelib| path.starts_with(corelib));
        let hint = if inCorelib(firstRoot) != inCorelib(&root) {
            "; one copy comes from the installed Corelib closure that is attached implicitly as `Core`. \
             Declare the Corelib aggregate that owns the explicit path (for example \
             `dependency \"corelib\" { source = path path = \"<workspace>/beskid_corelib\" }`) \
             or remove the explicit path dependency"
        } else {
            "; point both dependency declarations at the same package or rename one dependency label"
        };
        return Err(ProjectError::Validation(format!(
            "dependency `{}` resolves to two different packages: {} and {}{hint}",
            dependency.dependency_name,
            first.manifest_path.display(),
            dependency.manifest_path.display(),
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::RejectAmbiguousDependencyNames;
    use crate::projects::{ProjectError, ResolvedDependencyProject};
    use std::fs;
    use std::path::Path;

    fn dependency(name: &str, root: &Path) -> ResolvedDependencyProject {
        fs::create_dir_all(root.join("src")).unwrap();
        ResolvedDependencyProject {
            dependency_name: name.to_string(),
            manifest_path: root.join(format!("{name}.bproj")),
            project_root: root.to_path_buf(),
            project_name: name.to_string(),
            source_root: root.join("src"),
        }
    }

    #[test]
    fn same_package_reached_twice_is_one_identity() {
        let workspace = tempfile::tempdir().unwrap();
        let foundation = workspace.path().join("packages/foundation");
        let direct = dependency("corelib_foundation", &foundation);
        let viaAlias = dependency("corelib_foundation", &workspace.path().join("packages/../packages/foundation"));
        RejectAmbiguousDependencyNames(&[direct, viaAlias], None).unwrap();
    }

    #[test]
    fn two_packages_claiming_one_name_fail_closed_with_both_roots() {
        let checkout = tempfile::tempdir().unwrap();
        let installed = tempfile::tempdir().unwrap();
        let explicit = dependency("corelib_foundation", &checkout.path().join("packages/foundation"));
        let implicit = dependency("corelib_foundation", &installed.path().join("packages/foundation"));
        let error = RejectAmbiguousDependencyNames(&[explicit.clone(), implicit.clone()], Some(installed.path()))
            .unwrap_err();
        let ProjectError::Validation(message) = error else { panic!("expected a validation error") };
        assert!(message.contains("`corelib_foundation` resolves to two different packages"), "{message}");
        assert!(message.contains(&explicit.manifest_path.display().to_string()), "{message}");
        assert!(message.contains(&implicit.manifest_path.display().to_string()), "{message}");
        assert!(message.contains("implicitly as `Core`"), "{message}");
    }

    mod registry_closure {
        use super::super::{RegistryRequest, ResolveRegistryClosure};
        use crate::projects::{DependencySource, ProjectError, UnresolvedDependencyNote};
        use std::collections::BTreeMap;

        /// A fake registry: `name@version` -> requirements as `(name, version)`.
        type Packages = BTreeMap<&'static str, Vec<(&'static str, &'static str)>>;

        fn request(name: &str, version: &str, requester: &str, direct: bool) -> RegistryRequest {
            RegistryRequest::New(
                UnresolvedDependencyNote {
                    dependency_name: name.to_owned(),
                    source: DependencySource::Registry,
                    descriptor: version.to_owned(),
                    requester: requester.to_owned(),
                },
                direct,
            )
        }

        fn resolve(
            packages: &Packages,
            roots: &[(&str, &str)],
            resolved: &mut Vec<String>,
        ) -> Result<Vec<String>, ProjectError> {
            let roots = roots.iter().map(|(name, version)| request(name, version, "App", true)).collect();
            let closure = ResolveRegistryClosure(roots, |request| {
                let coordinate = format!("{}@{}", request.Name(), request.version);
                resolved.push(coordinate.clone());
                let Some(requires) = packages.get(coordinate.as_str()) else { return Ok(None) };
                Ok(Some((
                    coordinate.clone(),
                    requires.iter().map(|(name, version)| request_of(name, version, &coordinate)).collect(),
                )))
            })?;
            Ok(closure.resolved.into_iter().map(|(_, coordinate)| coordinate).collect())
        }

        fn request_of(name: &str, version: &str, requester: &str) -> RegistryRequest {
            request(name, version, requester, false)
        }

        #[test]
        fn diamond_resolves_the_shared_package_once() {
            let packages = Packages::from([
                ("Alpha@1.0.0", vec![("Shared", "1.0.0")]),
                ("Beta@1.0.0", vec![("Shared", "1.0.0")]),
                ("Shared@1.0.0", vec![]),
            ]);
            let mut calls = Vec::new();
            let closure = resolve(&packages, &[("Alpha", "1.0.0"), ("Beta", "1.0.0")], &mut calls).unwrap();
            assert_eq!(closure, ["Alpha@1.0.0", "Beta@1.0.0", "Shared@1.0.0"]);
            assert_eq!(calls.iter().filter(|call| *call == "Shared@1.0.0").count(), 1, "one selection per package");
        }

        #[test]
        fn conflicting_versions_fail_closed_naming_both_requesters_and_versions() {
            let packages = Packages::from([
                ("Alpha@1.0.0", vec![("Shared", "1.0.0")]),
                ("Beta@1.0.0", vec![("Shared", "2.0.0")]),
                ("Shared@1.0.0", vec![]),
                ("Shared@2.0.0", vec![]),
            ]);
            let error = resolve(&packages, &[("Alpha", "1.0.0"), ("Beta", "1.0.0")], &mut Vec::new()).unwrap_err();
            let message = error.to_string();
            for expected in ["`Alpha@1.0.0` requires `Shared@1.0.0`", "`Beta@1.0.0` requires `Shared@2.0.0`", "override"] {
                assert!(message.contains(expected), "{expected} in {message}");
            }
        }

        #[test]
        fn identical_requests_from_two_requesters_share_one_package() {
            let mut calls = Vec::new();
            let roots = vec![request("Shared", "1.0.0", "App", true), request("Shared", "1.0.0", "Lib", true)];
            let closure = ResolveRegistryClosure(roots, |request| {
                calls.push(request.note.requester.clone());
                Ok(Some(((), Vec::new())))
            })
            .unwrap();
            assert_eq!(closure.resolved.len(), 1);
            assert_eq!(calls, ["App"], "one selection per package");
        }

        #[test]
        fn one_requester_declaring_a_package_twice_fails_closed_before_unifying() {
            let roots = vec![request("Shared", "1.0.0", "App", true), request("Shared", "1.0.0", "App", true)];
            let Err(error) = ResolveRegistryClosure(roots, |_| Ok(Some(((), Vec::new())))) else {
                panic!("a requester declaring one package twice must fail closed");
            };
            let message = error.to_string();
            assert!(message.contains("`App` declares registry package `Shared` twice"), "{message}");
            assert!(message.contains("materialized destination"), "{message}");
        }

        #[test]
        fn direct_and_transitive_requests_must_agree() {
            let packages = Packages::from([("Alpha@1.0.0", vec![("Shared", "2.0.0")]), ("Shared@1.0.0", vec![])]);
            let message =
                resolve(&packages, &[("Alpha", "1.0.0"), ("Shared", "1.0.0")], &mut Vec::new()).unwrap_err().to_string();
            assert!(message.contains("`App` requires `Shared@1.0.0`"), "{message}");
            assert!(message.contains("`Alpha@1.0.0` requires `Shared@2.0.0`"), "{message}");
        }

        #[test]
        fn requirement_cycles_are_rejected_with_the_cycle_named() {
            let packages = Packages::from([
                ("Alpha@1.0.0", vec![("Beta", "1.0.0")]),
                ("Beta@1.0.0", vec![("Gamma", "1.0.0")]),
                ("Gamma@1.0.0", vec![("Alpha", "1.0.0")]),
            ]);
            let mut calls = Vec::new();
            let error = resolve(&packages, &[("Alpha", "1.0.0")], &mut calls).unwrap_err();
            assert!(matches!(error, ProjectError::DependencyCycle(_)), "{error}");
            assert!(error.to_string().contains("Alpha -> Beta -> Gamma -> Alpha"), "{error}");
            assert_eq!(calls.len(), 3, "resolution terminates: each package is selected once");
        }

        #[test]
        fn removing_one_direct_keeps_the_shared_transitive() {
            let packages = Packages::from([
                ("Alpha@1.0.0", vec![("Shared", "1.0.0"), ("OnlyAlpha", "1.0.0")]),
                ("Beta@1.0.0", vec![("Shared", "1.0.0")]),
                ("Shared@1.0.0", vec![]),
                ("OnlyAlpha@1.0.0", vec![]),
            ]);
            let both = resolve(&packages, &[("Alpha", "1.0.0"), ("Beta", "1.0.0")], &mut Vec::new()).unwrap();
            assert_eq!(both, ["Alpha@1.0.0", "Beta@1.0.0", "OnlyAlpha@1.0.0", "Shared@1.0.0"]);
            let remaining = resolve(&packages, &[("Beta", "1.0.0")], &mut Vec::new()).unwrap();
            assert_eq!(remaining, ["Beta@1.0.0", "Shared@1.0.0"], "unreachable packages drop, shared ones stay");
        }

        #[test]
        fn an_unavailable_transitive_fails_even_when_a_direct_request_is_warning_only() {
            // `Shared` is declared directly but unavailable (warning-only); a package requiring it
            // cannot be satisfied.
            let packages = Packages::from([("Alpha@1.0.0", vec![("Shared", "1.0.0")])]);
            let message =
                resolve(&packages, &[("Shared", "1.0.0"), ("Alpha", "1.0.0")], &mut Vec::new()).unwrap_err().to_string();
            assert!(message.contains("required by `Alpha@1.0.0` is unavailable"), "{message}");
        }

        #[test]
        fn pins_are_stale_only_when_registry_or_exact_version_differs() {
            use crate::projects::workflow::{ProjectLockDependencyEntry, ProjectLockSource};
            let pin = |registry: &str, version: &str| ProjectLockDependencyEntry {
                name: "Shared".into(),
                source: ProjectLockSource::Registry,
                manifest: "Shared.bproj".into(),
                project: "obj/beskid/deps/src/x".into(),
                source_root: "src".into(),
                materialized_root: "obj/beskid/deps/src/x".into(),
                resolved_version: Some(version.into()),
                artifact_digest: Some(format!("sha256:{}", "0".repeat(64))),
                registry: Some(registry.into()),
            };
            let exact = request("Shared", "1.0.0", "App", true);
            assert!(!exact.PinIsStale(&pin("default", "1.0.0")));
            assert!(exact.PinIsStale(&pin("default", "2.0.0")));
            assert!(exact.PinIsStale(&pin("mirror", "1.0.0")));
            assert!(!request("Shared", "*", "App", true).PinIsStale(&pin("default", "2.0.0")));
        }
    }
}
