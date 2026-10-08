//! Leaf-function root elision.
//!
//! Lowering registers a root slot for every managed local and parameter at binding time
//! (`gc_register_root`) and releases it on every exit edge (`gc_unregister_root`). A root only
//! matters while a collection can run, and a collection runs only inside a runtime call made by
//! the thread that owns the heap: the heap is per-thread state, collection is stop-the-world on
//! that thread, and it is triggered only by allocation or an explicit collect, both of which are
//! calls. Root registration itself never allocates through the GC or collects.
//!
//! A function whose only calls are root registrations therefore cannot reach a safepoint while
//! any of its roots is registered, and every root it registers is released before it returns.
//! Removing those calls (and the `trapz` that checks a registration result) cannot change which
//! objects are reachable at any collection. Every other function keeps its roots unchanged.
//!
//! Module emission applies this to every lowered program function once lowering is complete;
//! single-function ISLE emission keeps the unelided root protocol so it stays observable.

use cranelift_codegen::ir::{ExternalName, Function, Inst, InstructionData, Opcode};

const REGISTER_ROOT: &[u8] = b"gc_register_root";
const UNREGISTER_ROOT: &[u8] = b"gc_unregister_root";

/// Remove root registration from `function` when it makes no other call. Returns the number of
/// removed root calls (zero when the function is not a leaf or registers no roots).
pub fn elide_leaf_function_roots(function: &mut Function) -> usize {
    let mut registers = Vec::new();
    let mut unregisters = Vec::new();
    for block in function.layout.blocks() {
        for instruction in function.layout.block_insts(block) {
            let data = &function.dfg.insts[instruction];
            if !data.opcode().is_call() {
                continue;
            }
            let InstructionData::Call { opcode: Opcode::Call, func_ref, .. } = *data else {
                return 0;
            };
            let ExternalName::TestCase(name) = &function.dfg.ext_funcs[func_ref].name else {
                return 0;
            };
            match name.raw() {
                REGISTER_ROOT => registers.push(instruction),
                UNREGISTER_ROOT => unregisters.push(instruction),
                _ => return 0,
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
    use super::elide_leaf_function_roots;

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
}
