use beskid_codegen::{CodegenArtifact, LoweredFunction};
use cranelift_codegen::cursor::{Cursor, FuncCursor};
use cranelift_codegen::ir::{self, InstBuilder, types};
use cranelift_codegen::isa::CallConv;
use object::{Object, ObjectSection, ObjectSymbol, RelocationTarget};

use super::{BeskidObjectModule, BuildProfile};

fn caller(name: &str, call_conv: CallConv, managed: bool) -> LoweredFunction {
    let pointer = types::I64;
    let mut function = ir::Function::new();
    function.signature = ir::Signature::new(call_conv);
    function.signature.params.push(ir::AbiParam::new(pointer));
    function.signature.returns.push(ir::AbiParam::new(pointer));
    let signature = function.import_signature(ir::Signature::new(call_conv));
    let target = function.import_function(ir::ExtFuncData {
        name: ir::ExternalName::testcase("MapProbeTarget"),
        signature,
        colocated: true,
        patchable: false,
    });
    let block = function.dfg.make_block();
    let input = function.dfg.append_block_param(block, pointer);
    let mut cursor = FuncCursor::new(&mut function);
    cursor.insert_block(block);
    if managed {
        let slot = cursor.func.create_sized_stack_slot(ir::StackSlotData::new(ir::StackSlotKind::ExplicitSlot, 8, 3));
        cursor.ins().stack_store(pointer, input, slot, 0);
        let call = cursor.ins().call(target, &[]);
        cursor.func.dfg.append_user_stack_map_entry(call, ir::UserStackMapEntry { ty: pointer, slot, offset: 0 });
        let output = cursor.ins().stack_load(pointer, pointer, slot, 0);
        cursor.ins().return_(&[output]);
    } else {
        cursor.ins().call(target, &[]);
        cursor.ins().return_(&[input]);
    }
    LoweredFunction { name: name.to_owned(), function }
}

fn artifact(call_conv: CallConv) -> CodegenArtifact {
    let mut target = ir::Function::new();
    target.signature = ir::Signature::new(call_conv);
    let block = target.dfg.make_block();
    target.layout.append_block(block);
    FuncCursor::new(&mut target).at_bottom(block).ins().return_(&[]);
    CodegenArtifact {
        functions: vec![
            LoweredFunction { name: "MapProbeTarget".to_owned(), function: target },
            caller("ManagedCaller", call_conv, true),
            caller("IntegerCaller", call_conv, false),
        ],
        ..Default::default()
    }
}

fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

#[test]
fn object_stack_maps_preserve_code_relocations_and_pointer_slots() {
    let directory = tempfile::tempdir().unwrap();
    let targets = [
        ("x86_64-unknown-linux-gnu", ".data.rel.ro.beskid_gc", CallConv::SystemV),
        ("x86_64-pc-windows-msvc", ".rdata$GC", CallConv::WindowsFastcall),
        ("aarch64-apple-darwin", "__beskid_gc", CallConv::AppleAarch64),
    ];
    for (triple, section_name, call_conv) in targets {
        for profile in [BuildProfile::Debug, BuildProfile::Release] {
            let mut module = BeskidObjectModule::new(Some(triple), profile).unwrap();
            module.compile_artifact(&artifact(call_conv), None).unwrap();
            let path = directory.path().join("stack-maps.o");
            module.finalize_to_path(&path).unwrap();
            let bytes = std::fs::read(path).unwrap();
            let object = object::File::parse(bytes.as_slice()).unwrap();
            let mut sections = object.sections().filter(|section| section.name() == Ok(section_name));
            let section = sections.next().unwrap_or_else(|| panic!("missing {section_name} for {triple}"));
            assert!(sections.next().is_none(), "the raw integer caller must not get a stack map section");
            let map = section.data().unwrap();
            assert_eq!(&map[..8], b"BSKGC\0\x01\0");
            assert_eq!(word(map, 8) as usize, map.len());
            let code_size = word(map, 12);
            let mut relocations = section.relocations();
            let (offset, relocation) = relocations.next().expect("function address needs relocation");
            assert_eq!(offset, 16);
            let RelocationTarget::Symbol(target_symbol) = relocation.target() else {
                panic!("expected the managed function symbol");
            };
            let target = object.symbol_by_index(target_symbol).unwrap();
            let managed = object
                .symbols()
                .find(|symbol| matches!(symbol.name(), Ok("ManagedCaller" | "_ManagedCaller")))
                .unwrap();
            if object.format() != object::BinaryFormat::MachO {
                assert!(!target.is_global(), "the relocation must use a local code label for {triple}");
            }
            assert_eq!(target.address(), managed.address());
            assert_eq!(target.section_index(), managed.section_index());
            assert!(relocations.next().is_none());
            // The metadata covers both the inserted checkpoint and the preceding IR's call.
            assert_eq!(word(map, 24), 2);
            let mut previous_pc = 0;
            for site in [28, 44] {
                let pc = word(map, site);
                assert!(pc > previous_pc && pc < code_size);
                let span = word(map, site + 4);
                assert_eq!(word(map, site + 8), 1);
                let root_offset = word(map, site + 12);
                assert_eq!(root_offset % 8, 0);
                assert!(root_offset + 8 <= span);
                previous_pc = pc;
            }
            assert_eq!(&map[60..], &[0; 4]);
        }
    }
}
