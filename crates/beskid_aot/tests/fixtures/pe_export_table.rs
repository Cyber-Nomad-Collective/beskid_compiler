#![no_std]

// Source for pe_export_table.dll. Compile this and pe_import_provider.rs for
// x86_64-pc-windows-msvc, link the provider DLL/import library first, then link with:
// rust-lld -flavor link /dll /noentry /export:expected_export /out:pe_export_table.dll \
//   pe_export_table.obj pe_import_provider.lib
unsafe extern "C" {
    fn expected_import();
}

#[unsafe(no_mangle)]
pub extern "C" fn expected_export() {
    unsafe { expected_import() }
}
