//! [`run_rules`](core::run_rules) orchestrates [`Rule`](core::Rule) passes over a parsed [`Program`](crate::syntax::Program).

pub mod core;
pub mod resolve;
pub mod staged;
pub mod types;
pub use core::{AnalysisOptions, AnalysisResult, Rule, RuleContext, run_rules};
