mod cli_cross_platform;

#[cfg(target_os = "linux")]
mod aot_smoke;

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod crypto_baseline;

#[cfg(target_os = "linux")]
mod dependency_workflow;

#[cfg(target_os = "linux")]
mod failure_contracts;

#[cfg(target_os = "linux")]
mod performance;

#[cfg(target_os = "linux")]
mod runtime_cases;

#[cfg(target_os = "linux")]
mod runtime_linkage;

#[cfg(target_os = "linux")]
mod semantic_matrix;

#[cfg(target_os = "linux")]
mod workflow_matrix;

#[cfg(target_os = "linux")]
mod toolchain_gaps;
