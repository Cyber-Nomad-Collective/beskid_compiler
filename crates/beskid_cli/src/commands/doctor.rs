//! Read-only installation observations.
#![allow(non_snake_case)]
use crate::machine_output::Records;
use anyhow::Result;
use clap::Args;

#[derive(Args, Debug)]
pub struct DoctorArgs {
    /// Print schema-1 newline-delimited JSON records (`finding`, then one `result`) on stdout
    #[arg(long)]
    pub json: bool,
}

const REPAIR: &str = "run `beskid toolchain update` or install a complete toolchain bundle";

/// One observed installation component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub component: String,
    /// `ok`, `missing` or `invalid`.
    pub status: &'static str,
    pub detail: String,
    /// Next valid action for a problem finding.
    pub next: Option<&'static str>,
}

impl Finding {
    fn ok(component: impl Into<String>, detail: impl Into<String>) -> Self {
        Self { component: component.into(), status: "ok", detail: detail.into(), next: None }
    }
    fn problem(component: impl Into<String>, status: &'static str, detail: impl Into<String>) -> Self {
        Self { component: component.into(), status, detail: detail.into(), next: Some(REPAIR) }
    }
    pub fn IsProblem(&self) -> bool {
        self.status != "ok"
    }
}

/// Observe the host target, installed prefix, both runtime kit profiles and the Corelib bundle
/// without installing, provisioning or touching credentials.
pub fn Observe() -> Result<Vec<Finding>> {
    let target = beskid_abi::runtime_kit::host_runtime_target()?;
    let mut findings = vec![Finding::ok("host", target.triple.as_str())];
    let prefix = match beskid_abi::runtime_kit::installed_toolchain_prefix() {
        Ok(prefix) => {
            findings.push(Finding::ok("installation prefix", prefix.display().to_string()));
            prefix
        }
        Err(error) => {
            findings.push(Finding::problem(
                "installation prefix",
                "missing",
                format!("installation prefix unavailable: {error}"),
            ));
            return Ok(findings);
        }
    };
    for profile in [beskid_abi::runtime_kit::BuildProfile::Debug, beskid_abi::runtime_kit::BuildProfile::Release] {
        let component = format!("runtime kit {profile:?}");
        findings.push(match beskid_abi::runtime_kit::resolve_installed_runtime_kit(&prefix, &target, profile) {
            Ok(_) => Finding::ok(component, "verified"),
            Err(error) => Finding::problem(component, "invalid", format!("{error:?}")),
        });
    }
    let corelib = prefix.join("beskid_corelib");
    let verified = beskid_abi::corelib_bundle::verified_corelib_bundle_root(&corelib)
        .is_some_and(|verified| corelib.canonicalize().ok().as_ref() == Some(&verified));
    findings.push(if verified {
        Finding::ok("Corelib", "verified")
    } else if corelib.exists() {
        Finding::problem("Corelib", "invalid", format!("Corelib bundle at {} is invalid", corelib.display()))
    } else {
        Finding::problem("Corelib", "missing", format!("Corelib bundle missing at {}", corelib.display()))
    });
    Ok(findings)
}

pub fn execute(args: DoctorArgs) -> Result<()> {
    if args.json {
        return Records::new("doctor").Run(|records| {
            let findings = Observe()?;
            for finding in &findings {
                records.Emit(
                    "finding",
                    finding.status,
                    serde_json::json!({
                        "component": finding.component,
                        "detail": finding.detail,
                        "next": finding.next,
                    }),
                )?;
            }
            Verdict(&findings)?;
            Ok(serde_json::json!({ "findings": findings.len() }))
        });
    }
    let findings = Observe()?;
    for finding in &findings {
        if finding.IsProblem() {
            eprintln!("doctor: {} {}: {}; {}", finding.component, finding.status, finding.detail, REPAIR);
        } else {
            println!("{}: {}", finding.component, finding.detail);
        }
    }
    Verdict(&findings)
}

fn Verdict(findings: &[Finding]) -> Result<()> {
    let problems = findings.iter().filter(|finding| finding.IsProblem()).count();
    if problems != 0 {
        anyhow::bail!("doctor: {problems} installation problem(s) found; {REPAIR}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v06_doctor_problem_findings_name_next_action() {
        let finding = Finding::problem("Corelib", "missing", "Corelib bundle missing at /x");
        assert!(finding.IsProblem());
        assert_eq!(finding.next, Some(REPAIR));
        assert!(REPAIR.contains("beskid toolchain update"));
        assert!(!Finding::ok("host", "x86_64-unknown-linux-gnu").IsProblem());
        let error = Verdict(&[finding]).unwrap_err().to_string();
        assert!(error.contains("1 installation problem") && error.contains("toolchain update"), "{error}");
        assert!(Verdict(&[Finding::ok("host", "t")]).is_ok());
    }
}
