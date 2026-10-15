//! Invocation-scoped bounded values for CABI2 workers; no managed source pointers in transport.
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
const MAX_VALUES: usize = 1_000_000;
const MAX_BYTES: usize = 64 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub(crate) enum WireValue {
    Unit,
    Bool { value: bool },
    Signed { bits: u64 },
    Unsigned { bits: u64 },
    FloatBits { bits: u64 },
    String { value: String },
    Sequence { type_id: u32, values: Vec<u64> },
    Record { type_id: u32, values: Vec<u64> },
    Variant { type_id: u32, ordinal: u32, values: Vec<u64> },
}
impl WireValue {
    pub fn kind(&self) -> u32 {
        match self {
            Self::Unit => 0,
            Self::Bool { .. } => 1,
            Self::Signed { .. } => 2,
            Self::Unsigned { .. } => 3,
            Self::FloatBits { .. } => 4,
            Self::String { .. } => 5,
            Self::Sequence { .. } => 6,
            Self::Record { .. } => 7,
            Self::Variant { .. } => 8,
        }
    }
    fn children(&self) -> Option<&[u64]> {
        match self {
            Self::Sequence { values, .. } | Self::Record { values, .. } | Self::Variant { values, .. } => Some(values),
            _ => None,
        }
    }
}
pub(crate) struct WireArena {
    values: Vec<WireValue>,
    bytes: usize,
}
impl WireArena {
    pub fn new() -> Self {
        Self { values: Vec::new(), bytes: 0 }
    }
    pub fn get(&self, handle: u64) -> Result<&WireValue> {
        let index = usize::try_from(handle.checked_sub(1).context("null native value handle")?)?;
        self.values.get(index).context("foreign or stale native value handle")
    }
    pub fn insert(&mut self, value: WireValue) -> Result<u64> {
        if self.values.len() >= MAX_VALUES {
            bail!("native value count limit exceeded");
        }
        let added = match &value {
            WireValue::String { value } => value.len(),
            _ => value.children().map_or(0, |values| values.len().saturating_mul(8)),
        };
        let bytes = self.bytes.checked_add(added).context("native value size overflow")?;
        if bytes > MAX_BYTES {
            bail!("native value byte limit exceeded");
        }
        if let Some(children) = value.children() {
            for child in children {
                self.get(*child)?;
            }
        }
        self.bytes = bytes;
        self.values.push(value);
        Ok(u64::try_from(self.values.len())?)
    }
    pub fn child(&self, handle: u64, ordinal: usize) -> Result<u64> {
        self.get(handle)?
            .children()
            .context("native scalar has no children")?
            .get(ordinal)
            .copied()
            .context("native child ordinal out of bounds")
    }
    pub fn scalar(&self, handle: u64) -> Result<u64> {
        Ok(match self.get(handle)? {
            WireValue::Unit => 0,
            WireValue::Bool { value } => u64::from(*value),
            WireValue::Signed { bits } | WireValue::Unsigned { bits } | WireValue::FloatBits { bits } => *bits,
            _ => bail!("native compound value is not scalar"),
        })
    }
    /// Reverse construction only references prior values. This both prevents cycles and
    /// ensures recursive result walking remains bounded before any host AST translation.
    pub fn validate_tree(&self, root: u64) -> Result<()> {
        let mut pending = vec![(root, 0usize)];
        let mut visited = std::collections::HashSet::new();
        while let Some((handle, depth)) = pending.pop() {
            if depth > 128 {
                bail!("native result depth exceeded");
            }
            if !visited.insert(handle) {
                continue;
            }
            if visited.len() > MAX_VALUES {
                bail!("native result work exceeded");
            }
            if let Some(children) = self.get(handle)?.children() {
                pending.extend(children.iter().map(|child| (*child, depth + 1)));
            }
        }
        Ok(())
    }
}

#[repr(C)]
pub(crate) struct NativeHeaderV2 {
    pub version: u32,
    pub bytes: u32,
    pub generation: u64,
    pub invocation: u64,
    pub context: *mut std::ffi::c_void,
    pub callbacks: *const NativeCallbacksV2,
    pub request: u64,
    pub factory_request: u64,
}
#[repr(C)]
pub(crate) struct NativeCallbacksV2 {
    pub kind: unsafe extern "C" fn(*mut std::ffi::c_void, u64, *mut u32) -> i32,
    pub scalar: unsafe extern "C" fn(*mut std::ffi::c_void, u64, *mut u64) -> i32,
    pub text: unsafe extern "C" fn(*mut std::ffi::c_void, u64, *mut *const u8, *mut usize) -> i32,
    pub count: unsafe extern "C" fn(*mut std::ffi::c_void, u64, *mut usize) -> i32,
    pub child: unsafe extern "C" fn(*mut std::ffi::c_void, u64, usize, *mut u64) -> i32,
    pub variant: unsafe extern "C" fn(*mut std::ffi::c_void, u64, *mut u32) -> i32,
    pub new_scalar: unsafe extern "C" fn(*mut std::ffi::c_void, u32, u64, *mut u64) -> i32,
    pub new_text: unsafe extern "C" fn(*mut std::ffi::c_void, *const u8, usize, *mut u64) -> i32,
    pub new_sequence: unsafe extern "C" fn(*mut std::ffi::c_void, u32, *const u64, usize, *mut u64) -> i32,
    pub new_variant: unsafe extern "C" fn(*mut std::ffi::c_void, u32, u32, *const u64, usize, *mut u64) -> i32,
    pub service: unsafe extern "C" fn(*mut std::ffi::c_void, *const u8, usize, *const u64, usize, *mut u64) -> i32,
}
pub(crate) type NativeEntryV2 = unsafe extern "C" fn(*const NativeHeaderV2, *mut u64) -> i32;

pub(crate) struct WorkerValues<'a> {
    pub arena: WireArena,
    pub record_types: std::collections::HashSet<u32>,
    pub service: &'a mut dyn FnMut(&str, &[u64], &mut WireArena) -> Result<u64>,
    pub error: Option<String>,
}
fn boundary(context: *mut std::ffi::c_void, action: impl FnOnce(&mut WorkerValues<'_>) -> Result<()>) -> i32 {
    if context.is_null() {
        return 1;
    }
    // This pointer is the worker-owned invocation frame; no parent semantic pointer is
    // transported. It remains borrowed synchronously until the native entry returns.
    let state = unsafe { &mut *context.cast::<WorkerValues<'_>>() };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| action(state)));
    match result {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => {
            state.error = Some(error.to_string());
            1
        }
        Err(_) => {
            state.error = Some("native worker callback panicked".into());
            1
        }
    }
}
unsafe fn out<T>(pointer: *mut T, value: T) -> Result<()> {
    if pointer.is_null() {
        bail!("native callback null output");
    }
    unsafe { pointer.write(value) };
    Ok(())
}
unsafe extern "C" fn kind(c: *mut std::ffi::c_void, h: u64, o: *mut u32) -> i32 {
    boundary(c, |s| unsafe { out(o, s.arena.get(h)?.kind()) })
}
unsafe extern "C" fn scalar(c: *mut std::ffi::c_void, h: u64, o: *mut u64) -> i32 {
    boundary(c, |s| unsafe { out(o, s.arena.scalar(h)?) })
}
unsafe extern "C" fn text(c: *mut std::ffi::c_void, h: u64, p: *mut *const u8, n: *mut usize) -> i32 {
    boundary(c, |s| {
        let WireValue::String { value } = s.arena.get(h)? else {
            bail!("native value is not UTF8 text");
        };
        unsafe {
            out(p, value.as_ptr())?;
            out(n, value.len())
        }
    })
}
unsafe extern "C" fn count(c: *mut std::ffi::c_void, h: u64, o: *mut usize) -> i32 {
    boundary(c, |s| unsafe { out(o, s.arena.get(h)?.children().context("native scalar count")?.len()) })
}
unsafe extern "C" fn child(c: *mut std::ffi::c_void, h: u64, i: usize, o: *mut u64) -> i32 {
    boundary(c, |s| unsafe { out(o, s.arena.child(h, i)?) })
}
unsafe extern "C" fn variant(c: *mut std::ffi::c_void, h: u64, o: *mut u32) -> i32 {
    boundary(c, |s| {
        let WireValue::Variant { ordinal, .. } = s.arena.get(h)? else {
            bail!("native value is not variant");
        };
        unsafe { out(o, *ordinal) }
    })
}
unsafe extern "C" fn new_scalar(c: *mut std::ffi::c_void, k: u32, b: u64, o: *mut u64) -> i32 {
    boundary(c, |s| {
        let value = match k {
            0 if b == 0 => WireValue::Unit,
            1 if b <= 1 => WireValue::Bool { value: b == 1 },
            2 => WireValue::Signed { bits: b },
            3 => WireValue::Unsigned { bits: b },
            4 => WireValue::FloatBits { bits: b },
            _ => bail!("native scalar kind or bits invalid"),
        };
        unsafe { out(o, s.arena.insert(value)?) }
    })
}
unsafe extern "C" fn new_text(c: *mut std::ffi::c_void, p: *const u8, n: usize, o: *mut u64) -> i32 {
    boundary(c, |s| {
        if n > MAX_BYTES || (n != 0 && p.is_null()) {
            bail!("native UTF8 text bounds invalid");
        }
        let bytes = if n == 0 { &[][..] } else { unsafe { std::slice::from_raw_parts(p, n) } };
        let value = std::str::from_utf8(bytes)?.to_owned();
        unsafe { out(o, s.arena.insert(WireValue::String { value })?) }
    })
}
unsafe fn handles(p: *const u64, n: usize) -> Result<Vec<u64>> {
    if n > MAX_VALUES || (n != 0 && (p.is_null() || !p.is_aligned())) {
        bail!("native handle vector bounds invalid");
    }
    Ok(if n == 0 { Vec::new() } else { unsafe { std::slice::from_raw_parts(p, n) }.to_vec() })
}
unsafe extern "C" fn new_sequence(c: *mut std::ffi::c_void, t: u32, p: *const u64, n: usize, o: *mut u64) -> i32 {
    boundary(c, |s| {
        let values = unsafe { handles(p, n) }?;
        let value = if s.record_types.contains(&t) {
            WireValue::Record { type_id: t, values }
        } else {
            WireValue::Sequence { type_id: t, values }
        };
        unsafe { out(o, s.arena.insert(value)?) }
    })
}
unsafe extern "C" fn new_variant(
    c: *mut std::ffi::c_void,
    t: u32,
    v: u32,
    p: *const u64,
    n: usize,
    o: *mut u64,
) -> i32 {
    boundary(c, |s| unsafe {
        out(o, s.arena.insert(WireValue::Variant { type_id: t, ordinal: v, values: handles(p, n)? })?)
    })
}
unsafe extern "C" fn service(
    c: *mut std::ffi::c_void,
    p: *const u8,
    n: usize,
    a: *const u64,
    l: usize,
    o: *mut u64,
) -> i32 {
    boundary(c, |s| {
        if n > 1024 || p.is_null() {
            bail!("native callback operation bounds invalid");
        }
        let operation = std::str::from_utf8(unsafe { std::slice::from_raw_parts(p, n) })?;
        let arguments = unsafe { handles(a, l) }?;
        for argument in &arguments {
            s.arena.get(*argument)?;
        }
        let handle = (s.service)(operation, &arguments, &mut s.arena)?;
        s.arena.get(handle)?;
        unsafe { out(o, handle) }
    })
}
pub(crate) static NATIVE_CALLBACKS_V2: NativeCallbacksV2 = NativeCallbacksV2 {
    kind,
    scalar,
    text,
    count,
    child,
    variant,
    new_scalar,
    new_text,
    new_sequence,
    new_variant,
    service,
};

#[derive(Clone, Debug)]
pub(crate) enum WireTypeBody {
    Scalar(u32),
    Array(u32),
    Record(Vec<(String, u32)>),
    Enum(Vec<(String, Vec<(String, u32)>)>),
}
#[derive(Clone, Debug)]
pub(crate) struct WireType {
    pub name: String,
    pub body: WireTypeBody,
}
pub(crate) struct WireTypes {
    pub types: Vec<WireType>,
}
impl WireTypes {
    /// Call only after descriptor inventory and current-source closure verification. This
    /// parser is correspondence, never native producer/admission authority by itself.
    pub fn read(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > 16 * 1024 * 1024 {
            bail!("native adapter plan exceeds byte bound");
        }
        let plan: serde_json::Value = serde_json::from_slice(bytes)?;
        let array =
            plan.get("types").and_then(serde_json::Value::as_array).context("native adapter plan lacks types")?;
        if array.len() > 65536 {
            bail!("native adapter type count exceeded");
        }
        fn number(v: &serde_json::Value) -> Result<u32> {
            Ok(u32::try_from(v.as_u64().context("native type index invalid")?)?)
        }
        fn fields(v: &serde_json::Value) -> Result<Vec<(String, u32)>> {
            let array = v.as_array().context("native fields invalid")?;
            if array.len() > 65536 {
                bail!("native field count exceeded");
            }
            array
                .iter()
                .map(|field| {
                    Ok((
                        field
                            .get("name")
                            .and_then(serde_json::Value::as_str)
                            .context("native field name absent")?
                            .to_owned(),
                        number(field.get("type_id").context("native field type absent")?)?,
                    ))
                })
                .collect()
        }
        let mut types = Vec::new();
        for (ordinal, ty) in array.iter().enumerate() {
            if number(ty.get("id").context("native type id absent")?)? as usize != ordinal {
                bail!("native type IDs noncanonical");
            }
            let name =
                ty.get("name").and_then(serde_json::Value::as_str).context("native type name absent")?.to_owned();
            let kind = ty.get("kind").and_then(serde_json::Value::as_object).context("native type kind absent")?;
            if kind.len() != 1 {
                bail!("ambiguous native type body");
            }
            let (tag, payload) = kind.iter().next().unwrap();
            let body = match tag.as_str() {
                "Scalar" => WireTypeBody::Scalar(number(payload)?),
                "Array" => WireTypeBody::Array(number(payload.get("element").context("native array element absent")?)?),
                "Record" => {
                    WireTypeBody::Record(fields(payload.get("fields").context("native record fields absent")?)?)
                }
                "Enum" => {
                    let variants = payload
                        .get("variants")
                        .and_then(serde_json::Value::as_array)
                        .context("native variants absent")?;
                    let mut projected = Vec::new();
                    for (index, variant) in variants.iter().enumerate() {
                        if variant.get("tag").and_then(serde_json::Value::as_u64) != Some(index as u64) {
                            bail!("native variant tags noncanonical");
                        }
                        projected.push((
                            variant
                                .get("name")
                                .and_then(serde_json::Value::as_str)
                                .context("native variant name absent")?
                                .to_owned(),
                            fields(variant.get("fields").context("native variant fields absent")?)?,
                        ));
                    }
                    WireTypeBody::Enum(projected)
                }
                _ => bail!("native type body unsupported"),
            };
            types.push(WireType { name, body });
        }
        for ty in &types {
            let refs = match &ty.body {
                WireTypeBody::Scalar(_) => Vec::new(),
                WireTypeBody::Array(element) => vec![*element],
                WireTypeBody::Record(fields) => fields.iter().map(|(_, ty)| *ty).collect(),
                WireTypeBody::Enum(variants) => {
                    variants.iter().flat_map(|(_, fields)| fields.iter().map(|(_, ty)| *ty)).collect()
                }
            };
            if refs.iter().any(|reference| *reference as usize >= types.len()) {
                bail!("native type graph has dangling edge");
            }
        }
        Ok(Self { types })
    }
    pub fn record_types(&self) -> std::collections::HashSet<u32> {
        self.types
            .iter()
            .enumerate()
            .filter(|(_, ty)| matches!(ty.body, WireTypeBody::Record(_)))
            .map(|(ordinal, _)| ordinal as u32)
            .collect()
    }
}
impl WireTypes {
    pub fn build(&self, arena: &mut WireArena, ty: u32, value: &serde_json::Value) -> Result<u64> {
        self.build_at(arena, ty, value, 0)
    }
    fn build_at(&self, arena: &mut WireArena, ty: u32, value: &serde_json::Value, depth: usize) -> Result<u64> {
        if depth > 128 {
            bail!("native input tree depth exceeded");
        }
        let kind = &self.types.get(ty as usize).context("native input type absent")?.body;
        let value = match kind {
            WireTypeBody::Scalar(scalar) => match *scalar {
                0 if value.is_null() => WireValue::Unit,
                1 => WireValue::Bool { value: value.as_bool().context("native bool input invalid")? },
                2 | 3 | 12 | 13 => {
                    WireValue::Signed { bits: value.as_i64().context("native signed input invalid")? as u64 }
                }
                4 | 8 | 11 | 14 | 15 => {
                    WireValue::Unsigned { bits: value.as_u64().context("native unsigned input invalid")? }
                }
                5 => WireValue::FloatBits { bits: value.as_f64().context("native f64 input invalid")?.to_bits() },
                16 => WireValue::FloatBits {
                    bits: (value.as_f64().context("native f32 input invalid")? as f32).to_bits() as u64,
                },
                6 => {
                    let mut chars = value.as_str().context("native char input invalid")?.chars();
                    let scalar = chars.next().context("native char empty")?;
                    if chars.next().is_some() {
                        bail!("native char has multiple scalars");
                    }
                    WireValue::Unsigned { bits: scalar as u64 }
                }
                7 => WireValue::String { value: value.as_str().context("native string input invalid")?.to_owned() },
                _ => bail!("native raw address/bottom/unknown scalar input forbidden"),
            },
            WireTypeBody::Array(element) => {
                let array = value.as_array().context("native source array input invalid")?;
                if array.len() > MAX_VALUES {
                    bail!("native array input count exceeded");
                }
                let values = array
                    .iter()
                    .map(|value| self.build_at(arena, *element, value, depth + 1))
                    .collect::<Result<Vec<_>>>()?;
                WireValue::Sequence { type_id: ty, values }
            }
            WireTypeBody::Record(fields) => {
                let object = value.as_object().context("native source record input invalid")?;
                if object.len() != fields.len() {
                    bail!("native source record fields differ from exact signature");
                }
                let values = fields
                    .iter()
                    .map(|(name, ty)| {
                        self.build_at(
                            arena,
                            *ty,
                            object.get(name).with_context(|| format!("native source field {name} absent"))?,
                            depth + 1,
                        )
                    })
                    .collect::<Result<Vec<_>>>()?;
                WireValue::Record { type_id: ty, values }
            }
            WireTypeBody::Enum(variants) => {
                let (name, payload) = if let Some(name) = value.as_str() {
                    (name, serde_json::Value::Null)
                } else {
                    let object = value.as_object().context("native variant input invalid")?;
                    if object.len() != 1 {
                        bail!("native enum input ambiguous");
                    }
                    let (name, payload) = object.iter().next().unwrap();
                    (name.as_str(), payload.clone())
                };
                let (ordinal, (_, fields)) = variants
                    .iter()
                    .enumerate()
                    .find(|(_, (candidate, _))| candidate == name)
                    .context("native variant input unknown")?;
                let mut values = Vec::new();
                if fields.is_empty() {
                    if !payload.is_null() {
                        bail!("native unit variant has payload");
                    }
                } else {
                    let object = payload.as_object().context("native variant payload requires exact named fields")?;
                    if object.len() != fields.len() {
                        bail!("native variant payload arity mismatch");
                    }
                    for (name, ty) in fields {
                        values.push(self.build_at(
                            arena,
                            *ty,
                            object.get(name).context("native variant field absent")?,
                            depth + 1,
                        )?);
                    }
                }
                WireValue::Variant { type_id: ty, ordinal: u32::try_from(ordinal)?, values }
            }
        };
        arena.insert(value)
    }
    pub fn json(&self, arena: &WireArena, handle: u64, ty: u32) -> Result<serde_json::Value> {
        arena.validate_tree(handle)?;
        self.json_at(arena, handle, ty, 0)
    }
    fn json_at(&self, arena: &WireArena, handle: u64, ty: u32, depth: usize) -> Result<serde_json::Value> {
        if depth > 128 {
            bail!("native output tree depth exceeded");
        }
        let kind = &self.types.get(ty as usize).context("native result type absent")?.body;
        Ok(match (kind, arena.get(handle)?) {
            (WireTypeBody::Scalar(0), WireValue::Unit) => serde_json::Value::Null,
            (WireTypeBody::Scalar(1), WireValue::Bool { value }) => serde_json::json!(value),
            (WireTypeBody::Scalar(2 | 3 | 12 | 13), WireValue::Signed { bits }) => serde_json::json!(*bits as i64),
            (WireTypeBody::Scalar(4 | 8 | 11 | 14 | 15), WireValue::Unsigned { bits }) => serde_json::json!(bits),
            (WireTypeBody::Scalar(6), WireValue::Unsigned { bits }) => serde_json::json!(
                char::from_u32(u32::try_from(*bits)?).context("native result char invalid")?.to_string()
            ),
            (WireTypeBody::Scalar(7), WireValue::String { value }) => serde_json::json!(value),
            (WireTypeBody::Scalar(5 | 16), WireValue::FloatBits { bits }) => serde_json::json!({"ieeeBits":bits}),
            (WireTypeBody::Array(element), WireValue::Sequence { type_id, values }) if *type_id == ty => {
                serde_json::Value::Array(
                    values
                        .iter()
                        .map(|handle| self.json_at(arena, *handle, *element, depth + 1))
                        .collect::<Result<Vec<_>>>()?,
                )
            }
            (WireTypeBody::Record(fields), WireValue::Record { type_id, values })
                if *type_id == ty && fields.len() == values.len() =>
            {
                let mut object = serde_json::Map::new();
                for ((name, ty), handle) in fields.iter().zip(values) {
                    object.insert(name.clone(), self.json_at(arena, *handle, *ty, depth + 1)?);
                }
                serde_json::Value::Object(object)
            }
            (WireTypeBody::Enum(variants), WireValue::Variant { type_id, ordinal, values }) if *type_id == ty => {
                let (name, fields) = variants.get(*ordinal as usize).context("native result variant unknown")?;
                if fields.len() != values.len() {
                    bail!("native result variant arity mismatch");
                }
                if fields.is_empty() {
                    serde_json::json!(name)
                } else {
                    let mut payload = serde_json::Map::new();
                    for ((name, ty), handle) in fields.iter().zip(values) {
                        payload.insert(name.clone(), self.json_at(arena, *handle, *ty, depth + 1)?);
                    }
                    serde_json::json!({name:payload})
                }
            }
            _ => bail!("native result differs from exact compiler-issued source type"),
        })
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::*;
    #[test]
    #[cfg(target_pointer_width = "64")]
    fn v06_native_cabi2_header_matches_frozen_generated_c_transport() {
        use std::mem::{offset_of, size_of};
        assert_eq!(size_of::<NativeHeaderV2>(), 56);
        assert_eq!(offset_of!(NativeHeaderV2, generation), 8);
        assert_eq!(offset_of!(NativeHeaderV2, invocation), 16);
        assert_eq!(offset_of!(NativeHeaderV2, context), 24);
        assert_eq!(offset_of!(NativeHeaderV2, callbacks), 32);
        assert_eq!(offset_of!(NativeHeaderV2, request), 40);
        assert_eq!(offset_of!(NativeHeaderV2, factory_request), 48);
        assert_eq!(size_of::<NativeCallbacksV2>(), 88);
    }
    #[test]
    fn v06_native_cabi2_rejects_null_and_foreign_structural_handles_before_publication() {
        let mut arena = WireArena::new();
        assert!(arena.get(0).is_err());
        assert!(arena.insert(WireValue::Record { type_id: 7, values: vec![0] }).is_err());
        assert!(arena.insert(WireValue::Sequence { type_id: 8, values: vec![u64::MAX] }).is_err());
        let value = arena.insert(WireValue::Unsigned { bits: 7 }).unwrap();
        assert_eq!(value, 1, "rejected aggregate must not publish or reserve an arena handle");
        let record = arena.insert(WireValue::Record { type_id: 7, values: vec![value] }).unwrap();
        assert_eq!(arena.child(record, 0).unwrap(), value);
    }
}
