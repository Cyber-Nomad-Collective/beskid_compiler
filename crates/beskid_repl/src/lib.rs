//! Interactive snippet REPL backed by a persistent JIT [`beskid_engine::Engine`].
//!
//! v1 evaluates single-line expression or statement snippets by wrapping them as a `main`
//! entrypoint in synthetic source (no project manifest or module graph):
//!
//! - **Expressions** (e.g. `1 + 1`, `"hello"`) — wrapped as `<ret> main() { return …; }` with
//!   return type chosen by trying common scalar types until analysis succeeds.
//! - **Statements** (e.g. `let x = 1;`, `for i in range(0, 3) { … }`) — wrapped as
//!   `unit main() { … }`.
//!
//! Commands: `:quit` / `:q`, `:reset`, optional `:type <snippet>`.

pub mod eval;
pub mod readline;
pub mod session;

pub use session::ReplSession;

/// Synthetic source path passed to the analysis front-end (no filesystem or project resolve).
pub const REPL_SOURCE_PATH: &str = "<repl>";

/// Run the REPL loop on `input` until `:quit` or EOF.
pub fn run(session: &mut ReplSession, input: &mut dyn readline::ReplInput) -> anyhow::Result<()> {
    readline::run_loop(session, input).map_err(anyhow::Error::from)
}
