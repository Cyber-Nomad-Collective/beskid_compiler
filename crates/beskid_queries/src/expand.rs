//! Macro expansion helper (mirrors assembly loader).

pub(crate) fn expand_syntax_for_assembly(
    program: beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
) -> beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program> {
    beskid_analysis::macros::expand_program_with_diagnostics(
        program,
        beskid_analysis::macros::DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
        "",
        "",
    )
    .program
}
