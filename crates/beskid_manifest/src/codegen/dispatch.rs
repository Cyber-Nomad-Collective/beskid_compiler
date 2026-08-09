use std::fmt::Write as _;

use crate::model::{DispatchEntry, ManifestRoot};

use super::common::{dispatch_groups, dispatch_tag_imports, tag_const_name, write_generated_preamble};

fn runtime_dispatch_has_never_returns(manifest: &ManifestRoot) -> bool {
    dispatch_groups(manifest).into_iter().flatten().any(|entry| !entry.is_host() && entry.returns == "never")
}

fn dispatch_body_needs_i64_cast(dispatch_key: &str) -> bool {
    dispatch_key.starts_with("gc_")
        || matches!(
            dispatch_key,
            "str_eq"
                | "test_bytes_len"
                | "test_bytes_ptr"
                | "event_subscribe"
                | "event_unsubscribe_first"
                | "event_len"
                | "event_get_handler"
        )
}

fn dispatch_body_needs_usize_cast(dispatch_key: &str) -> bool {
    matches!(dispatch_key, "fs_write_text" | "syscall_write" | "syscall_write_bytes")
}

fn wrap_unsafe_body(body: &str) -> String {
    format!("unsafe {{\n{body}\n            }}")
}

pub(super) fn maybe_wrap_unsafe_body(body: &str) -> String {
    if body.contains("enum_ptr.add") { wrap_unsafe_body(body) } else { body.to_string() }
}

pub(super) fn wrap_dispatch_return(entry: &DispatchEntry, group: &str, body: &str) -> String {
    let body = maybe_wrap_unsafe_body(body);
    if entry.returns == "never" {
        return match group {
            "ptr" | "i64" => {
                format!("{{\n{body};\n            unreachable_unchecked()\n        }}")
            }
            _ => format!("Some({{\n{body};\n            unreachable_unchecked()\n        }})"),
        };
    }
    match group {
        "usize" if dispatch_body_needs_usize_cast(&entry.dispatch_key) => {
            format!("Some({{\n{body}\n        }} as usize)")
        }
        "usize" => format!("Some({body})"),
        "ptr" => format!("Some({{\n{body}\n        }} as *mut u8)"),
        "i64" if dispatch_body_needs_i64_cast(&entry.dispatch_key) => {
            format!("Some({{\n{body}\n        }} as i64)")
        }
        "i64" => format!("Some({body})"),
        _ => body.to_string(),
    }
}

pub fn render_runtime_dispatch_table(manifest: &ManifestRoot, rust_fallback_handlers: bool) -> String {
    let mut out = String::new();
    write_generated_preamble(&mut out, &["clippy::too_many_lines"]);
    if runtime_dispatch_has_never_returns(manifest) {
        writeln!(&mut out, "use core::hint::unreachable_unchecked;").unwrap();
    }
    writeln!(&mut out, "use beskid_abi::BeskidStr;").unwrap();
    writeln!(&mut out, "use beskid_abi::{{{}}};", dispatch_tag_imports(manifest)).unwrap();
    writeln!(&mut out).unwrap();

    let groups: [(&str, &[DispatchEntry], u32); 4] = [
        ("usize", &manifest.dispatch.usize, 0),
        ("ptr", &manifest.dispatch.ptr, 1),
        ("unit", &manifest.dispatch.unit, 2),
        ("i64", &manifest.dispatch.i64, 3),
    ];

    for (group, entries, group_id) in groups {
        writeln!(&mut out, "/// Dispatch return group `{group}` (registration group id `{group_id}`).").unwrap();
        writeln!(&mut out, "pub const DISPATCH_GROUP_{}: u32 = {group_id};", group.to_uppercase()).unwrap();
        let bitmap = valid_tags_bitmap(entries);
        writeln!(&mut out, "const VALID_TAGS_{}: u64 = {bitmap:#x};", group.to_uppercase()).unwrap();
        writeln!(&mut out).unwrap();
    }

    for (group, entries, _) in groups {
        render_valid_tag_fn(&mut out, group, entries);
    }
    writeln!(&mut out).unwrap();

    render_dispatch_fn(
        &mut out,
        "dispatch_usize",
        "usize",
        &manifest.dispatch.usize,
        "try_dispatch_usize",
        "Option<usize>",
        rust_fallback_handlers,
        |entry, body| wrap_dispatch_return(entry, "usize", body),
    );
    render_dispatch_fn(
        &mut out,
        "dispatch_ptr",
        "ptr",
        &manifest.dispatch.ptr,
        "try_dispatch_ptr",
        "Option<*mut u8>",
        rust_fallback_handlers,
        |entry, body| wrap_dispatch_return(entry, "ptr", body),
    );
    render_dispatch_fn(
        &mut out,
        "dispatch_unit",
        "unit",
        &manifest.dispatch.unit,
        "try_dispatch_unit",
        "bool",
        rust_fallback_handlers,
        |_entry, body| {
            let body = maybe_wrap_unsafe_body(body);
            format!("{{\n{body};\n            true\n        }}")
        },
    );
    render_dispatch_fn(
        &mut out,
        "dispatch_i64",
        "i64",
        &manifest.dispatch.i64,
        "try_dispatch_i64",
        "Option<i64>",
        rust_fallback_handlers,
        |entry, body| wrap_dispatch_return(entry, "i64", body),
    );

    out
}

fn valid_tags_bitmap(entries: &[DispatchEntry]) -> u64 {
    entries.iter().fold(0u64, |bits, entry| bits | (1u64 << entry.tag))
}

fn render_valid_tag_fn(out: &mut String, group: &str, entries: &[DispatchEntry]) {
    let upper = group.to_uppercase();
    writeln!(out, "fn is_valid_{group}_tag(tag: i32) -> bool {{").unwrap();
    writeln!(out, "    (0..64).contains(&tag) && (VALID_TAGS_{upper} & (1u64 << tag as u32)) != 0").unwrap();
    writeln!(out, "}}").unwrap();
    let _ = entries;
    writeln!(out).unwrap();
}

#[allow(clippy::too_many_arguments)]
fn render_dispatch_fn<F>(
    out: &mut String,
    fn_name: &str,
    group: &str,
    entries: &[DispatchEntry],
    try_override: &str,
    return_type: &str,
    rust_fallback_handlers: bool,
    wrap_body: F,
) where
    F: Fn(&DispatchEntry, &str) -> String,
{
    writeln!(out, "/// Dispatch `{group}` return group by manifest tag.").unwrap();
    writeln!(out, "///").unwrap();
    writeln!(out, "/// # Safety").unwrap();
    writeln!(out, "/// `enum_ptr` must reference a valid dispatch envelope for the duration of dispatch.").unwrap();
    writeln!(out, "pub unsafe fn {fn_name}(tag: i32, enum_ptr: *const u8) -> {return_type} {{").unwrap();
    writeln!(out, "    if !is_valid_{group}_tag(tag) {{").unwrap();
    match return_type {
        "bool" => writeln!(out, "        return false;").unwrap(),
        _ => writeln!(out, "        return None;").unwrap(),
    }
    writeln!(out, "    }}").unwrap();
    if return_type == "bool" {
        writeln!(out, "    if unsafe {{ crate::interop::register::{try_override}(tag, enum_ptr) }} {{").unwrap();
        writeln!(out, "        return true;").unwrap();
        writeln!(out, "    }}").unwrap();
    } else {
        writeln!(
            out,
            "    if let Some(value) = unsafe {{ crate::interop::register::{try_override}(tag, enum_ptr) }} {{"
        )
        .unwrap();
        writeln!(out, "        return Some(value);").unwrap();
        writeln!(out, "    }}").unwrap();
    }
    writeln!(out, "    match tag {{").unwrap();
    for entry in entries {
        if entry.is_host() {
            writeln!(
                out,
                "        TAG_{} => {{ crate::interop::register::trap_missing_host_handler(tag) }}",
                tag_const_name(&entry.name)
            )
            .unwrap();
            continue;
        }
        if entry.is_language_handler() && !rust_fallback_handlers {
            continue;
        }
        let body = render_dispatch_arm_body(entry, DispatchCallee::Runtime);
        let wrapped = wrap_body(entry, &body);
        writeln!(out, "        TAG_{} => {{ {wrapped} }}", tag_const_name(&entry.name)).unwrap();
    }
    match return_type {
        "bool" => writeln!(out, "        _ => false,").unwrap(),
        _ => writeln!(out, "        _ => None,").unwrap(),
    }
    writeln!(out, "    }}").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();
}

pub(super) fn host_wrapper_return_type(group: &str) -> &'static str {
    match group {
        "usize" => "usize",
        "ptr" => "*mut u8",
        "unit" => "()",
        "i64" => "i64",
        _ => "usize",
    }
}

#[derive(Copy, Clone)]
pub(super) enum DispatchCallee {
    Runtime,
    Host,
    Language,
}

pub(super) fn render_dispatch_arm_body(entry: &DispatchEntry, callee: DispatchCallee) -> String {
    if let Some(body) = special_dispatch_arm(entry, callee) {
        return body;
    }
    let mut decode = String::new();
    for (index, param) in entry.params.iter().enumerate() {
        let offset = 16 + index * 8;
        let var = format!("p{index}");
        let load = match callee {
            DispatchCallee::Language => language_envelope_load(param, index),
            _ => format!("*(enum_ptr.add({offset}) as {})", payload_load_type(param)),
        };
        writeln!(decode, "            let {var} = {load};").unwrap();
    }
    let args = entry
        .params
        .iter()
        .enumerate()
        .map(|(index, param)| cast_call_arg(&entry.dispatch_key, param, &format!("p{index}"), index))
        .collect::<Vec<_>>()
        .join(", ");
    let callee = dispatch_callee_path(&entry.dispatch_key, callee);
    if args.is_empty() {
        format!("{decode}            {callee}()")
    } else {
        format!("{decode}            {callee}({args})")
    }
}

fn dispatch_callee_path(dispatch_key: &str, callee: DispatchCallee) -> String {
    match callee {
        DispatchCallee::Host => format!("crate::{dispatch_key}"),
        DispatchCallee::Language => format!("crate::{dispatch_key}"),
        DispatchCallee::Runtime => match dispatch_key {
            "channel_receive_ptr" | "channel_send_ptr" | "channel_try_receive_ptr" | "channel_try_send_ptr" => {
                format!("crate::{dispatch_key}")
            }
            other => format!("crate::builtins::{other}"),
        },
    }
}

fn special_dispatch_arm(entry: &DispatchEntry, callee: DispatchCallee) -> Option<String> {
    let prefix = match callee {
        DispatchCallee::Host => "crate::",
        DispatchCallee::Language => "crate::",
        DispatchCallee::Runtime => "crate::builtins::",
    };
    match entry.dispatch_key.as_str() {
        "array_len" => Some(
            "            let p0 = *(enum_ptr.add(16) as *const *const u8);\n\
             crate::builtins::array_len(p0 as *const beskid_abi::BeskidArray)"
                .to_string(),
        ),
        "event_subscribe" => Some(
            "            let p0 = *(enum_ptr.add(16) as *const *const u8);\n\
             let p1 = *(enum_ptr.add(24) as *const *const u8);\n\
             crate::builtins::event_subscribe(\
                p0 as *mut *mut crate::builtins::EventState, p1 as *mut u8, 256)"
                .to_string(),
        ),
        "event_unsubscribe_first" => Some(
            "            let p0 = *(enum_ptr.add(16) as *const *const u8);\n\
             let p1 = *(enum_ptr.add(24) as *const *const u8);\n\
             crate::builtins::event_unsubscribe_first(\
                p0 as *mut *mut crate::builtins::EventState, p1 as *mut u8)"
                .to_string(),
        ),
        "event_len" => Some(
            "            let p0 = *(enum_ptr.add(16) as *const *const u8);\n\
             crate::builtins::event_len(p0 as *mut crate::builtins::EventState)"
                .to_string(),
        ),
        "event_get_handler" => Some(
            "            let p0 = *(enum_ptr.add(16) as *const *const u8);\n\
             let p1 = *(enum_ptr.add(24) as *const u64);\n\
             crate::builtins::event_get_handler(\
                p0 as *mut crate::builtins::EventState, p1 as usize)"
                .to_string(),
        ),
        "process_exit" => Some(format!(
            "            let p0 = *(enum_ptr.add(16) as *const u64);\n\
             {prefix}process_exit(p0 as i64);"
        )),
        "fiber_spawn" => Some(
            "            let p0 = *(enum_ptr.add(16) as *const *const u8);\n\
             let p1 = *(enum_ptr.add(24) as *const *const u8);\n\
             crate::builtins::fiber_spawn(\
                core::mem::transmute::<*const u8, extern \"C\" fn(*mut u8) -> i64>(p0), p1 as *mut u8)"
                .to_string(),
        ),
        "fiber_spawn_with_cancel_slot" => Some(
            "            let p0 = *(enum_ptr.add(16) as *const *const u8);\n\
             let p1 = *(enum_ptr.add(24) as *const *const u8);\n\
             let p2 = *(enum_ptr.add(32) as *const *const u8);\n\
             crate::builtins::fiber_spawn_with_cancel_slot(\
                core::mem::transmute::<*const u8, extern \"C\" fn(*mut u8) -> i64>(p0), p1 as *mut u8, \
                p2 as *mut *mut crate::builtins::EventState)"
                .to_string(),
        ),
        "str_concat" => Some(
            "            let p0 = *(enum_ptr.add(16) as *const *const u8);\n\
             let p1 = *(enum_ptr.add(24) as *const *const u8);\n\
             crate::builtins::str_concat(p0 as *const BeskidStr, p1 as *const BeskidStr)"
                .to_string(),
        ),
        "str_eq" => Some(
            "            let p0 = *(enum_ptr.add(16) as *const *const u8);\n\
             let p1 = *(enum_ptr.add(24) as *const *const u8);\n\
             crate::builtins::str_eq(p0 as *const BeskidStr, p1 as *const BeskidStr)"
                .to_string(),
        ),
        "channel_receive_ptr" | "channel_try_receive_ptr" => {
            let symbol = entry.dispatch_key.clone();
            Some(format!(
                "            let p0 = *(enum_ptr.add(16) as *const u64);\n\
                 let p1 = *(enum_ptr.add(24) as *const *const u8);\n\
                 crate::{symbol}(p0 as i64, p1 as *mut *mut u8)"
            ))
        }
        "channel_send_ptr" | "channel_try_send_ptr" => {
            let symbol = entry.dispatch_key.clone();
            Some(format!(
                "            let p0 = *(enum_ptr.add(16) as *const u64);\n\
                 let p1 = *(enum_ptr.add(24) as *const *const u8);\n\
                 crate::{symbol}(p0 as i64, p1 as *mut u8)"
            ))
        }
        _ => None,
    }
}

fn language_envelope_load(param: &str, param_index: usize) -> String {
    match param {
        "string" => format!("crate::envelope::load_string(enum_ptr, {param_index})"),
        "ptr" => format!("crate::envelope::load_ptr(enum_ptr, {param_index})"),
        "usize" => format!("crate::envelope::load_usize(enum_ptr, {param_index})"),
        "u64" => format!("crate::envelope::load_u64(enum_ptr, {param_index})"),
        "i64" => format!("crate::envelope::load_i64(enum_ptr, {param_index})"),
        "i32" => format!("crate::envelope::load_raw::<i32>(enum_ptr, {param_index})"),
        _ => format!("crate::envelope::load_u64(enum_ptr, {param_index})"),
    }
}

fn payload_load_type(param: &str) -> &'static str {
    match param {
        "string" => "*const *const BeskidStr",
        "ptr" => "*const *const u8",
        "usize" => "*const usize",
        "u64" => "*const u64",
        "i64" => "*const i64",
        "i32" => "*const i32",
        _ => "*const u64",
    }
}

fn cast_call_arg(dispatch_key: &str, param: &str, var: &str, index: usize) -> String {
    match param {
        "string" => var.to_string(),
        "ptr" => ptr_arg_cast(dispatch_key, var, index),
        "usize" => match dispatch_key {
            "array_new" if index == 0 => var.to_string(),
            "array_new" if index == 1 => var.to_string(),
            "str_new" if index == 1 => var.to_string(),
            "dynamic_object_alloc" if index == 0 => var.to_string(),
            "dynamic_cell_create" | "dynamic_cell_wrap" if index == 0 => {
                format!("{var} as u32")
            }
            "dynamic_cast_checked" if index == 1 => format!("{var} as u32"),
            _ => var.to_string(),
        },
        "u64" | "i64" | "i32" => numeric_arg_cast(dispatch_key, param, var, index),
        _ => var.to_string(),
    }
}

fn numeric_arg_cast(dispatch_key: &str, param: &str, var: &str, index: usize) -> String {
    match (dispatch_key, index) {
        ("str_from_i64", 0) => format!("{var} as i64"),
        ("str_slice", 1) | ("str_slice", 2) => format!("{var} as usize"),
        ("bytes_set", 1) | ("bytes_set", 2) => format!("{var} as i64"),
        ("dynamic_map_aot", 0)
        | ("dynamic_map_aot", 1)
        | ("dynamic_cast_checked", 1)
        | ("dynamic_cell_create", 0)
        | ("dynamic_cell_wrap", 0) => format!("{var} as u32"),
        _ => match param {
            "u64" if dispatch_key == "str_slice" && (index == 1 || index == 2) => {
                format!("{var} as usize")
            }
            "i64" => var.to_string(),
            "i32" => var.to_string(),
            "u64" => format!("{var} as i64"),
            _ => var.to_string(),
        },
    }
}

fn ptr_arg_cast(dispatch_key: &str, var: &str, index: usize) -> String {
    match (dispatch_key, index) {
        ("array_len", 0) => format!("{var} as *const beskid_abi::BeskidArray"),
        ("bytes_from_str", 0) | ("str_from_bytes_utf8", 0) => {
            format!("{var} as *const beskid_abi::BeskidArray")
        }
        ("bytes_compare", 0)
        | ("bytes_compare", 1)
        | ("bytes_copy", 0)
        | ("bytes_copy", 2)
        | ("bytes_get", 0)
        | ("bytes_set", 0)
        | ("syscall_write_bytes", 1) => {
            format!("{var} as *const beskid_abi::BeskidArray")
        }
        ("str_new", 0) => var.to_string(),
        ("str_concat", 0) | ("str_concat", 1) | ("str_eq", 0) | ("str_eq", 1) => {
            format!("{var} as *const BeskidStr")
        }
        ("dynamic_cell_create", 1)
        | ("dynamic_cell_wrap", 1)
        | ("dynamic_map_aot", 2)
        | ("dynamic_map_aot", 3)
        | ("dynamic_map_fallback", 0)
        | ("dynamic_map_fallback", 2)
        | ("dynamic_object_alloc", 0) => format!("{var} as *mut u8"),
        ("dynamic_cast_checked", 0) => format!("{var} as *mut crate::dynamic::DynamicCell"),
        ("channel_receive", 1) | ("channel_try_receive", 1) => format!("{var} as *mut i64"),
        ("fiber_spawn", 1) | ("fiber_spawn_with_cancel_slot", 1) | ("fiber_spawn_with_cancel_slot", 2) => {
            format!("{var} as *mut u8")
        }
        ("event_subscribe", 0) => {
            format!("{var} as *mut *mut crate::builtins::EventState")
        }
        ("event_unsubscribe_first", 0) => {
            format!("{var} as *mut *mut crate::builtins::EventState")
        }
        ("event_len", 0) | ("event_get_handler", 0) => {
            format!("{var} as *mut crate::builtins::EventState")
        }
        _ => format!("{var} as *mut u8"),
    }
}
