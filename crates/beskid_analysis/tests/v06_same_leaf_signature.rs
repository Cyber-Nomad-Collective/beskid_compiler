use beskid_analysis::resolve::Resolver;
use beskid_analysis::services::parse_program;
use beskid_analysis::types::checker::TypeChecker;
use std::path::PathBuf;

#[test]
fn dependency_signature_uses_its_own_same_leaf_type() {
    let bounded_path = PathBuf::from("/tmp/v06-same-leaf/Core/Text/Regex/Bounded.bd");
    let schema_path = PathBuf::from("/tmp/v06-same-leaf/Core/Bsol/Schema.bd");
    let entry_path = PathBuf::from("/tmp/v06-same-leaf/Main.bd");
    let bounded = parse_program("pub type PatternLimits { pub i64 work, } pub i64 Read(PatternLimits policy) { return policy.work; }").unwrap();
    let schema = parse_program("use Core.Text.Regex.Bounded; pub type PatternLimits { pub i64 depth, } pub i64 Validate(PatternLimits policy) { return policy.depth; }").unwrap();
    let mut entry = parse_program("use Core.Bsol.Schema; use Core.Text.Regex.Bounded; unit Main() { Schema.PatternLimits policy = Schema.PatternLimits { depth: 128_i64 }; Schema.Validate(policy); }").unwrap();
    let mut resolver = Resolver::new();
    resolver.collect_program_in_module(&bounded, &["Core".into(), "Text".into(), "Regex".into(), "Bounded".into()], Some(&bounded_path));
    resolver.collect_program_in_module(&schema, &["Core".into(), "Bsol".into(), "Schema".into()], Some(&schema_path));
    let resolution = resolver.resolve_program(&entry).expect("canonical imports and declarations resolve");
    let (_, errors) = TypeChecker::check_entry(&mut entry, &resolution, &[&bounded, &schema], Some(&[bounded_path, schema_path]), Some(entry_path), true, None, None, None, None);
    assert!(errors.is_empty(), "each signature must retain its lexical type declaration: {errors:?}");
}

#[test]
fn checked_project_entry_resolves_its_own_pattern_limits() {
    use beskid_analysis::projects::{AssemblyOptions, CompilePlan, ResolvedDependencyProject, Target, TargetKind};
    use beskid_analysis::projects::assembly::assemble_program_with_materializer;
    use beskid_analysis::services::{resolve_and_type_program_with_assembly, DependencyTypingPolicy};
    let temporary = tempfile::tempdir().unwrap();
    let host = temporary.path().join("schema");
    let dependency = temporary.path().join("bounded");
    let entry_path = host.join("src/Probe/Schema.bd");
    let bounded_path = dependency.join("src/Probe/Bounded.bd");
    std::fs::create_dir_all(entry_path.parent().unwrap()).unwrap();
    std::fs::create_dir_all(bounded_path.parent().unwrap()).unwrap();
    std::fs::write(&entry_path, "use Probe.Bounded;\npub type PatternLimits { pub i64 maxDepth, }\npub i64 Validate() { return ValidateWithPatternLimits(PatternLimits { maxDepth:64_i64 }); }\npub i64 ValidateWithPatternLimits(PatternLimits policy) { return Bounded.Read(Bounded.PatternLimits {maxWork:policy.maxDepth}); }\n").unwrap();
    std::fs::write(&bounded_path, "pub type PatternLimits { pub i64 maxWork, } pub i64 Read(PatternLimits policy) { return policy.maxWork; }").unwrap();
    let plan = CompilePlan {
        project_root: host.clone(), manifest_path: host.join("schema.bproj"), project_name: "schema".into(), source_root: host.join("src"),
        target: Target { name: "SchemaCheck".into(), kind: TargetKind::Lib, entry: Some("Probe/Schema.bd".into()) },
        dependency_projects: vec![ResolvedDependencyProject { dependency_name: "bounded".into(), manifest_path: dependency.join("bounded.bproj"), project_root: dependency.clone(), project_name: "bounded".into(), source_root: dependency.join("src") }],
        unresolved_dependencies: Vec::new(), has_core_dependency: false,
    };
    let assembly = assemble_program_with_materializer(&plan, None, &entry_path, None, &AssemblyOptions::default(), None, None).unwrap();
    let result = resolve_and_type_program_with_assembly(&assembly.entry_unit().program, Some(&assembly), None, DependencyTypingPolicy::FullClosure);
    assert!(result.is_ok(), "checked entry signature must use its own declaration: {result:?}");
}
