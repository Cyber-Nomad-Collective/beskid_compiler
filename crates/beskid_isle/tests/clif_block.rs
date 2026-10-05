use beskid_isle::{AstNodeKey, IsleContext, NodeFacts, NodeKind, lower_expression};
use beskid_queries::{AstNodeId, BeskidDatabase, ClifParameterShape, SourceUnitId, SyntaxGenerationId};
use cranelift_codegen::ir::{AbiParam, Function, InstBuilder, Signature, Type, types};
use cranelift_codegen::settings;
use cranelift_codegen::verify_function;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module};

struct ClifBlockFacts {
    block_node: AstNodeKey,
    body: String,
    result_type: Type,
    shapes: Option<Vec<ClifParameterShape>>,
}

impl NodeFacts for ClifBlockFacts {
    fn node_kind(&self, n: AstNodeKey) -> Option<NodeKind> {
        (n == self.block_node).then_some(NodeKind::ClifBlock)
    }
    fn clif_block_body(&self, n: AstNodeKey) -> Option<String> {
        (n == self.block_node).then(|| self.body.clone())
    }
    fn clif_block_parameters(&self, n: AstNodeKey) -> Option<Vec<ClifParameterShape>> {
        (n == self.block_node).then(|| self.shapes.clone()).flatten()
    }
    fn scalar_type(&self, n: AstNodeKey) -> Option<Type> {
        (n == self.block_node).then_some(self.result_type)
    }
    fn integer_literal(&self, _: AstNodeKey) -> Option<i64> {
        None
    }
}

/// Host ISA with detected CPU features, so SIMD lane operations select native lowerings.
fn make_isa() -> std::sync::Arc<dyn cranelift_codegen::isa::TargetIsa> {
    let mut flags = settings::builder();
    use cranelift_codegen::settings::Configurable;
    flags.set("is_pic", "false").expect("is_pic flag");
    cranelift_native::builder().expect("host ISA").finish(settings::Flags::new(flags)).unwrap()
}

fn make_key(db: &BeskidDatabase, id: u32) -> AstNodeKey {
    AstNodeKey { unit: SourceUnitId::new(db, "/tmp/M.bd".into()), generation: SyntaxGenerationId(1), node: AstNodeId(id) }
}

/// Lower `body` as the whole body of `fn(params) -> result`, returning the verified function or
/// the lowering error text.
fn lower(
    body: &str,
    params: &[Type],
    result: Type,
    shapes: Option<Vec<ClifParameterShape>>,
) -> Result<(Function, std::sync::Arc<dyn cranelift_codegen::isa::TargetIsa>), String> {
    let db = BeskidDatabase::default();
    let block_node = make_key(&db, 10);
    let facts = ClifBlockFacts { block_node, body: body.to_owned(), result_type: result, shapes };
    let isa = make_isa();
    let mut func = Function::with_name_signature(
        cranelift_codegen::ir::UserFuncName::user(0, 0),
        Signature {
            params: params.iter().map(|ty| AbiParam::new(*ty)).collect(),
            returns: vec![AbiParam::new(result)],
            call_conv: isa.default_call_conv(),
        },
    );
    let mut ctx = FunctionBuilderContext::new();
    {
        let mut b = FunctionBuilder::new(&mut func, &mut ctx);
        let entry = b.create_block();
        b.append_block_params_for_function_params(entry);
        b.switch_to_block(entry);
        b.seal_block(entry);
        let values = b.block_params(entry).to_vec();
        let mut c = IsleContext::new(&mut b, &facts, isa.frontend_config());
        c.function_param_values.extend(values);
        let value = lower_expression(&mut c, block_node).map_err(|error| error.to_string())?;
        b.ins().return_(&[value]);
        b.finalize(isa.frontend_config());
    }
    verify_function(&func, isa.flags()).map_err(|error| format!("final function does not verify: {error}"))?;
    Ok((func, isa))
}

/// JIT the lowered function and return its entry address with the owning module.
fn jit(func: Function, isa: std::sync::Arc<dyn cranelift_codegen::isa::TargetIsa>) -> (JITModule, *const u8) {
    let mut module = JITModule::new(JITBuilder::with_isa(isa, cranelift_module::default_libcall_names()));
    let id = module.declare_function("clif_block_test", Linkage::Export, &func.signature).expect("declare");
    let mut context = module.make_context();
    context.func = func;
    module.define_function(id, &mut context).expect("define");
    module.finalize_definitions().expect("finalize");
    let entry = module.get_finalized_function(id);
    (module, entry)
}

fn lower_ok(body: &str, params: &[Type], result: Type, shapes: Option<Vec<ClifParameterShape>>) -> Function {
    lower(body, params, result, shapes).unwrap_or_else(|error| panic!("{body}: {error}")).0
}

fn lower_err(body: &str, params: &[Type], result: Type, shapes: Option<Vec<ClifParameterShape>>) -> String {
    match lower(body, params, result, shapes) {
        Ok((func, _)) => panic!("{body} unexpectedly lowered:\n{}", func.display()),
        Err(error) => error,
    }
}

const SCALAR: ClifParameterShape = ClifParameterShape::Scalar;

#[test]
fn clif_block_call_emits_verified_clif() {
    let func = lower_ok("call @sqrt(%0)", &[types::F64], types::F64, None);
    assert!(func.display().to_string().contains("call"));
}

#[test]
fn clif_block_return_param_emits_verified_clif() {
    let func = lower_ok("return %0", &[types::F64], types::F64, None);
    assert!(func.display().to_string().contains("return"));
}

#[test]
fn clif_block_two_arg_call_emits_verified_clif() {
    let func = lower_ok("call @atan2(%0, %1)", &[types::F64, types::F64], types::F64, None);
    assert!(func.display().to_string().contains("call"));
}

#[test]
fn clif_block_typed_call_result_feeds_instructions() {
    let func = lower_ok(
        "%r = call @labs(%0) -> i64\n%s = iadd %r, %0\nreturn %s",
        &[types::I64],
        types::I64,
        Some(vec![SCALAR]),
    );
    let text = func.display().to_string();
    assert!(text.contains("call") && text.contains("iadd"), "{text}");
}

#[test]
fn clif_block_xor_runs() {
    let (func, isa) = lower("%x = bxor %0, %1\nreturn %x", &[types::I64, types::I64], types::I64, Some(vec![SCALAR; 2]))
        .expect("xor lowers");
    let (_module, entry) = jit(func, isa);
    let f: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(entry) };
    assert_eq!(f(0b1100, 0b1010), 0b0110);
}

#[test]
fn clif_block_umulhi_returns_high_product_word() {
    let (func, isa) =
        lower("%h = umulhi %0, %1 // high 64 bits\nreturn %h", &[types::I64, types::I64], types::I64, None)
            .expect("umulhi lowers");
    let (_module, entry) = jit(func, isa);
    let f: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(entry) };
    let (a, b) = (u64::MAX, 3_u64);
    assert_eq!(f(a as i64, b as i64) as u64, ((u128::from(a) * u128::from(b)) >> 64) as u64);
}

#[test]
fn clif_block_rotl_on_u32_runs() {
    let (func, isa) = lower("%r = rotl %0, %1\nreturn %r", &[types::I32, types::I32], types::I32, None).expect("rotl");
    let (_module, entry) = jit(func, isa);
    let f: extern "C" fn(u32, u32) -> u32 = unsafe { std::mem::transmute(entry) };
    assert_eq!(f(0x8000_0001, 4), 0x8000_0001_u32.rotate_left(4));
}

#[test]
fn clif_block_i32x4_simd_add_runs() {
    let body = "%a = splat.i32x4 %0\n%b = splat.i32x4 %1\n%c = insertlane %b, %0, 3\n%s = iadd %a, %c\n%l2 = \
                extractlane %s, 2\n%l3 = extractlane %s, 3\n%r = isub %l3, %l2\nreturn %r";
    let (func, isa) = lower(body, &[types::I32, types::I32], types::I32, None).expect("simd lowers");
    let (_module, entry) = jit(func, isa);
    let f: extern "C" fn(i32, i32) -> i32 = unsafe { std::mem::transmute(entry) };
    // lane2 = a + b, lane3 = a + a; difference = a - b.
    assert_eq!(f(40, 2), 38);
}

#[test]
fn clif_block_shuffle_and_carry_ops_verify() {
    lower_ok(
        "%a = splat.i8x16 %0\n%b = splat.i8x16 %1\n%s = shuffle %a, %b, \
         0x1f1e1d1c1b1a19181716151413121110\n%l = extractlane %s, 0\n%w = uextend.i32 %l\nreturn %w",
        &[types::I8, types::I8],
        types::I32,
        None,
    );
    lower_ok(
        "%lo, %c = uadd_overflow %0, %1\n%c64 = uextend.i64 %c\n%r = iadd %lo, %c64\nreturn %r",
        &[types::I64, types::I64],
        types::I64,
        None,
    );
}

/// The three-word ABI-v5 array header `{ ptr, len, cap }`.
#[repr(C)]
struct ArrayHeader {
    ptr: *mut u8,
    len: u64,
    cap: u64,
}

#[test]
fn clif_block_u32_payload_load_xor_store_runs() {
    let body = "%p = payload %0\n%four = iconst.i64 4\n%a = iadd %p, %four\n%w = load.i32 %a\n%y = bxor %w, \
                %1\nstore %y, %a\nistore8 %1, %p+12\nreturn %y";
    let (func, isa) = lower(
        body,
        &[types::I64, types::I32],
        types::I32,
        Some(vec![ClifParameterShape::PayloadArray { element_bytes: 4 }, SCALAR]),
    )
    .expect("payload block lowers");
    let (_module, entry) = jit(func, isa);
    let f: extern "C" fn(*const ArrayHeader, u32) -> u32 = unsafe { std::mem::transmute(entry) };
    let mut data = [1_u32, 0xF0F0_0000, 3, 0xFFFF_FFFF];
    let header = ArrayHeader { ptr: data.as_mut_ptr().cast(), len: 4, cap: 4 };
    assert_eq!(f(&header, 0x0000_FF0F), 0xF0F0_FF0F);
    assert_eq!(data, [1, 0xF0F0_FF0F, 3, 0xFFFF_FF0F]);
}

#[test]
fn clif_block_length_reads_array_header() {
    let (func, isa) = lower(
        "%n = length %0\nreturn %n",
        &[types::I64],
        types::I64,
        Some(vec![ClifParameterShape::PayloadArray { element_bytes: 8 }]),
    )
    .expect("length lowers");
    let (_module, entry) = jit(func, isa);
    let f: extern "C" fn(*const ArrayHeader) -> i64 = unsafe { std::mem::transmute(entry) };
    let mut data = [0_i64; 5];
    let header = ArrayHeader { ptr: data.as_mut_ptr().cast(), len: 5, cap: 5 };
    assert_eq!(f(&header), 5);
}

#[test]
fn clif_block_rejects_unsafe_memory_access() {
    let array = Some(vec![ClifParameterShape::PayloadArray { element_bytes: 1 }, SCALAR]);
    let error = lower_err("%v = load.i64 %1\nreturn %v", &[types::I64, types::I64], types::I64, array.clone());
    assert!(error.contains("must derive from `payload"), "{error}");
    let error = lower_err("%p = payload %0\nreturn %p", &[types::I64, types::I64], types::I64, array.clone());
    assert!(error.contains("cannot leave the block"), "{error}");
    let error = lower_err("%p = payload %0\nstore %p, %p\nreturn %1", &[types::I64, types::I64], types::I64, array.clone());
    assert!(error.contains("cannot be stored"), "{error}");
    let error = lower_err(
        "%p = payload %0\n%m = imul %p, %1\n%v = load.i64 %m\nreturn %v",
        &[types::I64, types::I64],
        types::I64,
        array.clone(),
    );
    assert!(error.contains("may only be offset"), "{error}");
    let error = lower_err("%x = iadd %0, %1\nreturn %x", &[types::I64, types::I64], types::I64, array.clone());
    assert!(error.contains("is an array"), "{error}");
    let error = lower_err("%p = payload %1\n%v = load.i64 %p\nreturn %v", &[types::I64, types::I64], types::I64, array);
    assert!(error.contains("`u8[]`, `u32[]`, or `i64[]`"), "{error}");
    let error = lower_err("%p = payload %0\n%v = load.i64 %p\nreturn %v", &[types::I64], types::I64, None);
    assert!(error.contains("`u8[]`, `u32[]`, or `i64[]`"), "{error}");
}

#[test]
fn clif_block_rejects_invalid_instructions() {
    let error = lower_err("%x = iadd %0, %1\nreturn %x", &[types::I64, types::I32], types::I64, None);
    assert!(error.contains("does not verify") && error.contains("%1"), "{error}");
    let error = lower_err("%x = ireduce.i32 %0\nreturn %x", &[types::I64], types::I64, None);
    assert!(error.contains("expects `i64`"), "{error}");
    let error = lower_err("%x = call_indirect %0\nreturn %x", &[types::I64], types::I64, None);
    assert!(error.contains("not allowed"), "{error}");
    let error = lower_err("%x = iadd %0, %3\nreturn %x", &[types::I64], types::I64, None);
    assert!(error.contains("does not name a parameter"), "{error}");
    let error = lower_err("%x = bogus_op %0\nreturn %x", &[types::I64], types::I64, None);
    assert!(error.contains("not allowed"), "{error}");
    let error = lower_err("%x = iadd %0\nreturn %x", &[types::I64], types::I64, None);
    assert!(error.contains("clif block line 1"), "{error}");
}
