//! Root elision for functions that cannot reach a collection.
//!
//! Lowering registers a root slot for every managed local and parameter at binding time
//! (`gc_register_root`) and releases it on every exit edge (`gc_unregister_root`). A root only
//! matters while a collection can run, and a collection runs only inside a runtime call made by
//! the thread that owns the heap: the heap is per-thread state, collection is stop-the-world on
//! that thread, and it is triggered only by allocation or an explicit collect, both of which are
//! calls. Root registration itself never allocates through the GC or collects.
//!
//! A function cannot reach a collection when every call it makes, other than root registration,
//! goes to a function that cannot reach one either: another function of the same module with that
//! property, or one of the few runtime services in [`NON_COLLECTING_IMPORTS`]. Any other import,
//! any indirect call and any call form other than a plain `call` can collect. Such a function
//! cannot reach a safepoint while any of its roots is registered, and every root it registers is
//! released before it returns, so removing those calls (and the `trapz` that checks a
//! registration result) cannot change which objects are reachable at any collection. Its callers
//! are unaffected: an argument they pass stays reachable for the whole call, because nothing in
//! the call can collect. Every other function keeps its roots unchanged.
//!
//! The analysis needs the whole module, so module emission applies it to every lowered program
//! function once lowering is complete; single-function ISLE emission keeps the unelided root
//! protocol so it stays observable. It is only sound where the collector itself lies outside the
//! module, which holds for every program module and not for the canonical runtime corpus.

use std::collections::{HashMap, HashSet};

use cranelift_codegen::ir::{ExternalName, Function, Inst, InstructionData, Opcode};

const REGISTER_ROOT: &[u8] = b"gc_register_root";
const UNREGISTER_ROOT: &[u8] = b"gc_unregister_root";

/// Runtime services that never allocate through the GC, collect or switch fibers: they only read
/// the string and array headers they are given and compare or copy plain bytes
/// (`Runtime/Data/Strings.bd`, `Runtime/Data/Collections.bd`).
pub const NON_COLLECTING_IMPORTS: &[&[u8]] =
    &[b"str_eq", b"str_cmp", b"str_len", b"array_len", b"bytes_compare", b"bytes_copy"];

/// The call targets of `function` other than root registration: `Some(names)` for direct calls
/// only, `None` when it makes an indirect call or uses another call form.
fn direct_call_targets(function: &Function) -> Option<Vec<&[u8]>> {
    let mut targets = Vec::new();
    for block in function.layout.blocks() {
        for instruction in function.layout.block_insts(block) {
            let data = &function.dfg.insts[instruction];
            if !data.opcode().is_call() {
                continue;
            }
            let InstructionData::Call { opcode: Opcode::Call, func_ref, .. } = *data else {
                return None;
            };
            let ExternalName::TestCase(name) = &function.dfg.ext_funcs[func_ref].name else {
                return None;
            };
            let name = name.raw();
            if name != REGISTER_ROOT && name != UNREGISTER_ROOT {
                targets.push(name);
            }
        }
    }
    Some(targets)
}

/// Names of the functions in `functions` (one module, keyed by the symbol their callers use)
/// that cannot reach a collection.
pub fn non_collecting_functions<'a>(functions: impl IntoIterator<Item = (&'a str, &'a Function)>) -> HashSet<String> {
    let mut targets: HashMap<&'a [u8], Option<Vec<&'a [u8]>>> = HashMap::new();
    for (name, function) in functions {
        targets.insert(name.as_bytes(), direct_call_targets(function));
    }
    // Greatest fixed point: start from every function and drop each one that calls an
    // import outside the allow-list, makes an indirect call, or calls a dropped function.
    let mut candidates: HashSet<&'a [u8]> = targets.keys().copied().collect();
    loop {
        let dropped = candidates
            .iter()
            .copied()
            .filter(|name| match &targets[name] {
                None => true,
                Some(callees) => callees.iter().any(|callee| {
                    !candidates.contains(callee) && !NON_COLLECTING_IMPORTS.contains(callee)
                }),
            })
            .collect::<Vec<_>>();
        if dropped.is_empty() {
            break;
        }
        for name in dropped {
            candidates.remove(name);
        }
    }
    candidates.into_iter().filter_map(|name| std::str::from_utf8(name).ok().map(str::to_owned)).collect()
}

/// Remove root registration from `function` when every call it makes, other than root
/// registration, goes to a member of `non_collecting` or of [`NON_COLLECTING_IMPORTS`].
/// Returns the number of removed root calls (zero when the function keeps its roots or
/// registers none). An empty `non_collecting` set elides roots only in leaf functions.
pub fn elide_non_collecting_function_roots(function: &mut Function, non_collecting: &HashSet<String>) -> usize {
    let Some(callees) = direct_call_targets(function) else {
        return 0;
    };
    if callees.iter().any(|callee| {
        !NON_COLLECTING_IMPORTS.contains(callee)
            && !std::str::from_utf8(callee).is_ok_and(|callee| non_collecting.contains(callee))
    }) {
        return 0;
    }
    let mut registers = Vec::new();
    let mut unregisters = Vec::new();
    for block in function.layout.blocks() {
        for instruction in function.layout.block_insts(block) {
            let InstructionData::Call { opcode: Opcode::Call, func_ref, .. } = function.dfg.insts[instruction] else {
                continue;
            };
            let ExternalName::TestCase(name) = &function.dfg.ext_funcs[func_ref].name else {
                continue;
            };
            match name.raw() {
                REGISTER_ROOT => registers.push(instruction),
                UNREGISTER_ROOT => unregisters.push(instruction),
                _ => {}
            }
        }
    }
    if registers.is_empty() && unregisters.is_empty() {
        return 0;
    }

    // A registration result may only feed the `trapz` that checks it.
    let mut checks: Vec<Inst> = Vec::new();
    for &register in &registers {
        let Some(&result) = function.dfg.inst_results(register).first() else {
            return 0;
        };
        for block in function.layout.blocks() {
            for instruction in function.layout.block_insts(block) {
                if instruction == register || !function.dfg.inst_values(instruction).any(|value| value == result) {
                    continue;
                }
                match function.dfg.insts[instruction] {
                    InstructionData::CondTrap { opcode: Opcode::Trapz, arg, .. } if arg == result => {
                        checks.push(instruction);
                    }
                    _ => return 0,
                }
            }
        }
    }

    for instruction in checks {
        function.layout.remove_inst(instruction);
    }
    let removed = registers.len() + unregisters.len();
    for instruction in registers.into_iter().chain(unregisters) {
        function.layout.remove_inst(instruction);
    }
    removed
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{elide_non_collecting_function_roots, non_collecting_functions};

    fn elide_leaf_function_roots(function: &mut cranelift_codegen::ir::Function) -> usize {
        elide_non_collecting_function_roots(function, &HashSet::new())
    }

    fn parse(text: &str) -> cranelift_codegen::ir::Function {
        cranelift_reader::parse_functions(text).expect("test CLIF parses").remove(0)
    }

    const LEAF: &str = "function %leaf(i64) -> i64 system_v {
    ss0 = explicit_slot 8, align = 8
    sig0 = (i64) -> i8 system_v
    sig1 = (i64) system_v
    fn0 = %gc_register_root sig0
    fn1 = %gc_unregister_root sig1

block0(v0: i64):
    v1 = stack_addr.i64 ss0
    store v0, v1
    v2 = call fn0(v1)
    trapz v2, user8
    v3 = load.i64 v0
    v4 = stack_addr.i64 ss0
    call fn1(v4)
    return v3
}";

    #[test]
    fn leaf_function_drops_root_registration() {
        let mut function = parse(LEAF);
        assert_eq!(elide_leaf_function_roots(&mut function), 2);
        let text = function.display().to_string();
        assert!(!text.contains("call fn") && !text.contains("trapz"), "{text}");
        cranelift_codegen::verify_function(&function, &cranelift_codegen::settings::Flags::new(cranelift_codegen::settings::builder()))
            .expect("elided leaf verifies");
    }

    #[test]
    fn function_with_another_call_keeps_its_roots() {
        let text = LEAF.replace(
            "    fn1 = %gc_unregister_root sig1\n",
            "    fn1 = %gc_unregister_root sig1\n    fn2 = %beskid_rt_v5_gc_collect sig1\n",
        )
        .replace("    v3 = load.i64 v0\n", "    call fn2(v0)\n    v3 = load.i64 v0\n");
        let mut function = parse(&text);
        assert_eq!(elide_leaf_function_roots(&mut function), 0);
        assert_eq!(function.display().to_string().matches("call fn").count(), 3);
    }

    const CALLER: &str = "function %caller(i64) -> i64 system_v {
    ss0 = explicit_slot 8, align = 8
    sig0 = (i64) -> i8 system_v
    sig1 = (i64) system_v
    sig2 = (i64) -> i64 system_v
    fn0 = %gc_register_root sig0
    fn1 = %gc_unregister_root sig1
    fn2 = %leaf sig2

block0(v0: i64):
    v1 = stack_addr.i64 ss0
    store v0, v1
    v2 = call fn0(v1)
    trapz v2, user8
    v3 = call fn2(v0)
    v4 = stack_addr.i64 ss0
    call fn1(v4)
    return v3
}";

    fn module(caller: &str) -> (cranelift_codegen::ir::Function, cranelift_codegen::ir::Function) {
        (parse(LEAF), parse(caller))
    }

    #[test]
    fn caller_of_non_collecting_functions_drops_its_roots() {
        let (leaf, mut caller) = module(CALLER);
        let non_collecting = non_collecting_functions([("leaf", &leaf), ("caller", &caller)]);
        assert_eq!(non_collecting, HashSet::from(["leaf".to_owned(), "caller".to_owned()]));
        assert_eq!(elide_non_collecting_function_roots(&mut caller, &non_collecting), 2);
        let text = caller.display().to_string();
        assert!(!text.contains("call fn0") && !text.contains("call fn1") && text.contains("call fn2"), "{text}");
        cranelift_codegen::verify_function(&caller, &cranelift_codegen::settings::Flags::new(cranelift_codegen::settings::builder()))
            .expect("elided caller verifies");
    }

    #[test]
    fn allow_listed_string_service_does_not_collect() {
        let (leaf, mut caller) = module(&CALLER.replace("%leaf sig2", "%str_len sig2"));
        let non_collecting = non_collecting_functions([("leaf", &leaf), ("caller", &caller)]);
        assert!(non_collecting.contains("caller"));
        assert_eq!(elide_non_collecting_function_roots(&mut caller, &non_collecting), 2);
    }

    #[test]
    fn collecting_import_propagates_to_every_caller() {
        let leaf = parse(&LEAF.replace(
            "    fn1 = %gc_unregister_root sig1\n",
            "    fn1 = %gc_unregister_root sig1\n    fn2 = %str_new sig1\n",
        )
        .replace("    v3 = load.i64 v0\n", "    call fn2(v0)\n    v3 = load.i64 v0\n"));
        let mut caller = parse(CALLER);
        let outer = parse(&CALLER.replace("%caller", "%outer").replace("%leaf sig2", "%caller sig2"));
        let non_collecting = non_collecting_functions([("leaf", &leaf), ("caller", &caller), ("outer", &outer)]);
        assert!(non_collecting.is_empty(), "{non_collecting:?}");
        assert_eq!(elide_non_collecting_function_roots(&mut caller, &non_collecting), 0);
    }

    #[test]
    fn unknown_callee_keeps_roots() {
        let (_, mut caller) = module(CALLER);
        let non_collecting = non_collecting_functions([("caller", &caller)]);
        assert!(non_collecting.is_empty());
        assert_eq!(elide_non_collecting_function_roots(&mut caller, &non_collecting), 0);
    }

    #[test]
    fn mutually_recursive_functions_without_allocation_do_not_collect() {
        let ping = parse(&CALLER.replace("%caller", "%ping").replace("%leaf sig2", "%pong sig2"));
        let pong = parse(&CALLER.replace("%caller", "%pong").replace("%leaf sig2", "%ping sig2"));
        let non_collecting = non_collecting_functions([("ping", &ping), ("pong", &pong)]);
        assert_eq!(non_collecting.len(), 2);
    }

    #[test]
    fn indirect_call_keeps_roots_even_with_non_collecting_module_functions() {
        let leaf = parse(LEAF);
        let mut caller = parse(&CALLER.replace("    v3 = call fn2(v0)\n", "    v3 = call_indirect sig2, v0(v0)\n"));
        let non_collecting = non_collecting_functions([("leaf", &leaf), ("caller", &caller)]);
        assert_eq!(non_collecting, HashSet::from(["leaf".to_owned()]));
        assert_eq!(elide_non_collecting_function_roots(&mut caller, &non_collecting), 0);
        assert_eq!(elide_non_collecting_function_roots(&mut caller, &HashSet::from(["caller".to_owned()])), 0);
    }

    /// Every runtime service that allocates, formats, writes or parks, like any other import outside
    /// the allow-list, can collect: a caller of one keeps its roots.
    #[test]
    fn allocating_and_io_runtime_services_count_as_collecting() {
        for service in ["str_new", "str_concat", "str_slice", "str_from_i64", "syscall_write", "beskid_trap_message", "unknown"] {
            let (leaf, mut caller) = module(&CALLER.replace("%leaf sig2", &format!("%{service} sig2")));
            let non_collecting = non_collecting_functions([("leaf", &leaf), ("caller", &caller)]);
            assert!(!non_collecting.contains("caller"), "{service}: {non_collecting:?}");
            assert_eq!(elide_non_collecting_function_roots(&mut caller, &non_collecting), 0, "{service}");
        }
    }
}
