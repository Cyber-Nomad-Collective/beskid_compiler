use std::fmt::Write as _;

use crate::model::{DispatchEntry, ManifestRoot};

use super::common::write_generated_preamble;
use super::dispatch::{DispatchCallee, host_wrapper_return_type, maybe_wrap_unsafe_body, render_dispatch_arm_body};

/// Generate host handler wrappers and registration table for `beskid_host`.
pub fn render_host_handler_table(manifest: &ManifestRoot) -> String {
    let mut out = String::new();
    write_generated_preamble(&mut out, &["clippy::too_many_lines"]);

    let groups: [(&str, &[DispatchEntry], u32); 4] = [
        ("usize", &manifest.dispatch.usize, 0),
        ("ptr", &manifest.dispatch.ptr, 1),
        ("unit", &manifest.dispatch.unit, 2),
        ("i64", &manifest.dispatch.i64, 3),
    ];

    let mut host_entries: Vec<(&str, &DispatchEntry, u32)> = Vec::new();
    for (group, entries, group_id) in &groups {
        for entry in *entries {
            if entry.is_host() {
                host_entries.push((group, entry, *group_id));
            }
        }
    }

    writeln!(&mut out, "use beskid_abi::BeskidStr;").unwrap();
    writeln!(&mut out, "use beskid_runtime::HandlerTableEntry;").unwrap();
    writeln!(&mut out).unwrap();

    for (group, entry, _) in &host_entries {
        let wrapper = host_wrapper_fn_name(entry);
        let return_type = host_wrapper_return_type(group);
        let enum_param = if entry.params.is_empty() { "_enum_ptr" } else { "enum_ptr" };
        let raw_body = render_dispatch_arm_body(entry, DispatchCallee::Host);
        let wrapped = if entry.returns == "never" {
            maybe_wrap_unsafe_body(&raw_body)
        } else if raw_body.contains("enum_ptr.add") {
            let inner = maybe_wrap_unsafe_body(&raw_body);
            match *group {
                "unit" => format!("{inner};"),
                "ptr" => format!("({inner}) as *mut u8"),
                "usize" => format!("({inner}) as usize"),
                "i64" => inner,
                _ => inner,
            }
        } else {
            match *group {
                "unit" => format!("{raw_body};"),
                "ptr" => format!("{raw_body} as *mut u8"),
                "usize" => format!("{raw_body} as usize"),
                "i64" => raw_body,
                _ => raw_body,
            }
        };
        writeln!(
            &mut out,
            "/// # Safety\n///\n/// `enum_ptr` must reference a valid dispatch envelope for the duration of the call."
        )
        .unwrap();
        writeln!(&mut out, "unsafe extern \"C\" fn {wrapper}({enum_param}: *const u8) -> {return_type} {{").unwrap();
        writeln!(&mut out, "    {wrapped}").unwrap();
        writeln!(&mut out, "}}").unwrap();
        writeln!(&mut out).unwrap();
    }

    writeln!(&mut out, "const HOST_HANDLERS: [HandlerTableEntry; {}] = [", host_entries.len()).unwrap();
    for (_, entry, group_id) in &host_entries {
        let wrapper = host_wrapper_fn_name(entry);
        writeln!(
            &mut out,
            "    HandlerTableEntry {{ group: {group_id}, tag: {}, fn_ptr: {wrapper} as *const u8 }},",
            entry.tag
        )
        .unwrap();
    }
    writeln!(&mut out, "];").unwrap();
    writeln!(&mut out).unwrap();
    writeln!(&mut out, "/// Register all host dispatch handlers with the language runtime.").unwrap();
    writeln!(&mut out, "#[unsafe(no_mangle)]").unwrap();
    writeln!(&mut out, "pub extern \"C-unwind\" fn beskid_host_register_all() -> i32 {{").unwrap();
    writeln!(&mut out, "    beskid_runtime::beskid_register_handlers(").unwrap();
    writeln!(&mut out, "        u64::from(beskid_abi::BESKID_RUNTIME_ABI_VERSION),").unwrap();
    writeln!(&mut out, "        HOST_HANDLERS.as_ptr(),").unwrap();
    writeln!(&mut out, "        HOST_HANDLERS.len() as u64,").unwrap();
    writeln!(&mut out, "    )").unwrap();
    writeln!(&mut out, "}}").unwrap();
    out
}

fn host_wrapper_fn_name(entry: &DispatchEntry) -> String {
    format!("host_dispatch_{}", entry.dispatch_key)
}
