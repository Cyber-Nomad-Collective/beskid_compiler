//! Child-only environment for the installed Windows native toolchain.

#[cfg(any(windows, test))]
use std::collections::BTreeMap;
#[cfg(any(windows, test))]
use std::ffi::OsString;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(any(windows, test))]
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(any(windows, test))]
use crate::error::AotError;
use crate::error::AotResult;

#[cfg(any(windows, test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct Instance {
    path: PathBuf,
    complete: bool,
    launchable: bool,
    version: String,
}

#[cfg(any(windows, test))]
fn unavailable(tool: &str, message: impl Into<String>) -> AotError {
    AotError::NativeToolUnavailable { tool: tool.to_owned(), message: message.into() }
}

#[cfg(any(windows, test))]
fn parse_instances(json: &str) -> AotResult<Vec<Instance>> {
    let values: Vec<serde_json::Value> = serde_json::from_str(json)
        .map_err(|error| unavailable("vswhere.exe", format!("cannot read installed VS instances: {error}")))?;
    Ok(values
        .iter()
        .filter_map(|value| {
            Some(Instance {
                path: PathBuf::from(value.get("installationPath")?.as_str()?),
                complete: value.get("isComplete").and_then(|v| v.as_bool()).unwrap_or(false),
                launchable: value.get("isLaunchable").and_then(|v| v.as_bool()).unwrap_or(false),
                version: value.get("installationVersion")?.as_str()?.to_owned(),
            })
        })
        .collect())
}

#[cfg(any(windows, test))]
fn select_instance(instances: &[Instance]) -> AotResult<PathBuf> {
    eligible_instances(instances)
        .first()
        .map(|instance| instance.path.clone())
        .ok_or_else(|| unavailable("VS 2022 Build Tools", "install the Desktop development with C++ workload"))
}

#[cfg(any(windows, test))]
fn eligible_instances(instances: &[Instance]) -> Vec<&Instance> {
    let mut eligible = instances
        .iter()
        .filter(|instance| instance.complete && instance.launchable && instance.version.starts_with("17."))
        .collect::<Vec<_>>();
    eligible.sort_by_key(|instance| {
        std::cmp::Reverse(instance.version.split('.').map(|part| part.parse::<u32>().unwrap_or(0)).collect::<Vec<_>>())
    });
    eligible
}

#[cfg(any(windows, test))]
fn parse_dev_output(output: &str) -> BTreeMap<String, OsString> {
    output
        .lines()
        .filter_map(|line| {
            let (name, value) = line.trim_end_matches('\r').split_once('=')?;
            if name.is_empty() || name.starts_with('=') {
                return None;
            }
            Some((name.to_ascii_uppercase(), OsString::from(value)))
        })
        .collect()
}

#[cfg(any(windows, test))]
fn paths(value: &OsString) -> impl Iterator<Item = PathBuf> + '_ {
    value
        .to_string_lossy()
        .split(';')
        .filter(|part| !part.is_empty())
        .map(PathBuf::from)
        .collect::<Vec<_>>()
        .into_iter()
}

#[cfg(any(windows, test))]
fn require_file_in_path(environment: &BTreeMap<String, OsString>, name: &str) -> AotResult<()> {
    let path = environment.get("PATH").ok_or_else(|| unavailable(name, "VsDevCmd.bat did not provide PATH"))?;
    if paths(path).any(|directory| directory.join(name).is_file()) {
        return Ok(());
    }
    Err(unavailable(name, format!("{name} is missing from the VS 2022 x64 toolchain")))
}

#[cfg(any(windows, test))]
fn require_one_of(environment: &BTreeMap<String, OsString>, names: &[&str]) -> AotResult<()> {
    let path = environment.get("PATH").expect("checked before tool validation");
    if names.iter().any(|name| paths(path).any(|directory| directory.join(name).is_file())) {
        return Ok(());
    }
    Err(unavailable(
        names.join(" or ").as_str(),
        format!("{} is missing from the native toolchain", names.join(" or ")),
    ))
}

#[cfg(any(windows, test))]
fn configure_from_dev_output(command: &mut Command, output: &str, llvm_bin: Option<&Path>) -> AotResult<()> {
    let mut environment = parse_dev_output(output);
    for name in ["PATH", "INCLUDE", "LIB", "WINDOWSSDKDIR", "WINDOWSSDKVERSION"] {
        if environment.get(name).is_none_or(|value| value.is_empty()) {
            return Err(unavailable(
                if name.starts_with("WINDOWSSDK") { "Windows SDK" } else { name },
                format!("VsDevCmd.bat did not provide {name}; install the Windows SDK and MSVC x64 tools"),
            ));
        }
    }
    require_file_in_path(&environment, "cl.exe")?;
    let lib = environment.get("LIB").expect("checked above");
    for name in ["kernel32.lib", "ucrt.lib", "msvcrt.lib"] {
        if !paths(lib).any(|directory| directory.join(name).is_file()) {
            return Err(unavailable(
                "Windows SDK import libraries",
                format!("{name} is missing from LIB; install the Windows SDK and MSVC x64 tools"),
            ));
        }
    }
    if let Some(directory) = llvm_bin.filter(|directory| directory.join("lld-link.exe").is_file()) {
        let path = environment.get_mut("PATH").expect("checked above");
        let mut value = path.to_string_lossy().into_owned();
        value.push(';');
        value.push_str(&directory.to_string_lossy());
        *path = OsString::from(value);
    }
    require_one_of(&environment, &["link.exe", "lld-link.exe"])?;
    require_one_of(&environment, &["lib.exe", "llvm-lib.exe"])?;
    // CreateProcess resolves a bare executable using the parent's PATH before the child's
    // environment is installed. Resolve it from the discovered PATH for ordinary shells.
    let program = command.get_program().to_string_lossy().into_owned();
    if Path::new(&program).components().count() == 1 {
        let executable =
            if program.to_ascii_lowercase().ends_with(".exe") { program } else { format!("{program}.exe") };
        if let Some(found) = paths(environment.get("PATH").expect("checked above"))
            .map(|directory| directory.join(&executable))
            .find(|candidate| candidate.is_file())
        {
            let previous = std::mem::replace(command, Command::new(found));
            command.args(previous.get_args());
            for (name, value) in previous.get_envs() {
                if let Some(value) = value {
                    command.env(name, value);
                } else {
                    command.env_remove(name);
                }
            }
            if let Some(directory) = previous.get_current_dir() {
                command.current_dir(directory);
            }
        }
    }
    command.envs(environment);
    Ok(())
}

#[cfg(any(windows, test))]
fn complete_explicit_environment(environment: &BTreeMap<String, OsString>) -> bool {
    ["PATH", "INCLUDE", "LIB"].iter().all(|name| environment.get(*name).is_some_and(|value| !value.is_empty()))
}

#[cfg(any(windows, test))]
fn configure_from_instances(
    command: &mut Command,
    instances: &[Instance],
    llvm_bin: Option<&Path>,
    mut run_developer_command: impl FnMut(&Path) -> AotResult<String>,
) -> AotResult<()> {
    select_instance(instances)?;
    let mut last_error = unavailable("VS 2022 Build Tools", "no complete x64 native toolchain was found");
    for instance in eligible_instances(instances) {
        match run_developer_command(&instance.path)
            .and_then(|output| configure_from_dev_output(command, &output, llvm_bin))
        {
            Ok(()) => return Ok(()),
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}

#[cfg(windows)]
fn installed_llvm_bin() -> Option<PathBuf> {
    ["ProgramFiles", "ProgramFiles(x86)"]
        .iter()
        .filter_map(std::env::var_os)
        .map(|root| PathBuf::from(root).join("LLVM").join("bin"))
        .find(|directory| directory.join("lld-link.exe").is_file())
}

#[cfg(windows)]
fn run_developer_command(instance: &Path) -> AotResult<String> {
    let developer_command = instance.join("Common7").join("Tools").join("VsDevCmd.bat");
    if !developer_command.is_file() {
        return Err(unavailable("VsDevCmd.bat", format!("missing in {}", instance.display())));
    }
    let invocation = format!("call \"{}\" -arch=x64 -host_arch=x64 >nul && set", developer_command.display());
    let output = Command::new("cmd.exe")
        .args(["/d", "/c"])
        .raw_arg(invocation)
        .output()
        .map_err(|error| unavailable("VsDevCmd.bat", error.to_string()))?;
    if !output.status.success() {
        return Err(unavailable("VsDevCmd.bat", String::from_utf8_lossy(&output.stderr).into_owned()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(windows)]
pub(crate) fn configure_windows_native_command(command: &mut Command) -> AotResult<()> {
    let explicit = std::env::vars_os()
        .map(|(name, value)| (name.to_string_lossy().to_ascii_uppercase(), value))
        .collect::<BTreeMap<_, _>>();
    if complete_explicit_environment(&explicit) {
        return Ok(());
    }
    let installer = std::env::var_os("ProgramFiles(x86)")
        .map(PathBuf::from)
        .ok_or_else(|| unavailable("vswhere.exe", "ProgramFiles(x86) is unavailable"))?
        .join("Microsoft Visual Studio")
        .join("Installer")
        .join("vswhere.exe");
    let output = Command::new(&installer)
        .args([
            "-all",
            "-version",
            "[17.0,18.0)",
            "-products",
            "*",
            "-requires",
            "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
            "-format",
            "json",
        ])
        .output()
        .map_err(|error| unavailable("vswhere.exe", format!("{}: {error}", installer.display())))?;
    if !output.status.success() {
        return Err(unavailable("vswhere.exe", String::from_utf8_lossy(&output.stderr).into_owned()));
    }
    let instances = parse_instances(&String::from_utf8_lossy(&output.stdout))?;
    configure_from_instances(command, &instances, installed_llvm_bin().as_deref(), run_developer_command)
}

#[cfg(not(windows))]
pub(crate) fn configure_windows_native_command(_command: &mut Command) -> AotResult<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::{
        Instance, complete_explicit_environment, configure_from_dev_output, configure_from_instances, parse_instances,
        select_instance,
    };

    #[test]
    fn selects_complete_build_tools_over_incomplete_instance() {
        let instances = parse_instances(
            r#"[{"installationPath":"C:\\Program Files\\Microsoft Visual Studio\\2022\\Community","installationVersion":"17.12.1","isComplete":false,"isLaunchable":true},{"installationPath":"C:\\Program Files\\Microsoft Visual Studio\\2022\\BuildTools","installationVersion":"17.11.4","isComplete":true,"isLaunchable":true}]"#,
        )
        .unwrap();
        assert_eq!(
            select_instance(&instances).unwrap().to_string_lossy(),
            r"C:\Program Files\Microsoft Visual Studio\2022\BuildTools"
        );
    }

    #[test]
    fn selects_newer_complete_instance_by_numeric_version() {
        let instances = parse_instances(
            r#"[{"installationPath":"C:\\VS\\old","installationVersion":"17.9.1","isComplete":true,"isLaunchable":true},{"installationPath":"C:\\VS\\new","installationVersion":"17.14.2","isComplete":true,"isLaunchable":true}]"#,
        )
        .unwrap();
        assert_eq!(select_instance(&instances).unwrap().to_string_lossy(), r"C:\VS\new");
    }

    #[test]
    fn developer_output_with_spaces_reaches_only_the_child() {
        let temp = tempfile::tempdir().unwrap();
        let tools = temp.path().join("Visual Studio Tools");
        let sdk = temp.path().join("Windows SDK Libraries");
        std::fs::create_dir_all(&tools).unwrap();
        std::fs::create_dir_all(&sdk).unwrap();
        for name in ["cl.exe", "link.exe", "lib.exe"] {
            std::fs::write(tools.join(name), "").unwrap();
        }
        for name in ["kernel32.lib", "ucrt.lib", "msvcrt.lib"] {
            std::fs::write(sdk.join(name), "").unwrap();
        }
        let output = format!(
            "PATH={}\r\nINCLUDE={}\r\nLIB={}\r\nWindowsSdkDir={}\r\nWindowsSDKVersion=10.0.26100.0\\\r\n",
            tools.display(),
            tools.display(),
            sdk.display(),
            temp.path().display()
        );
        let mut command = Command::new("cl");
        configure_from_dev_output(&mut command, &output, None).unwrap();
        assert_eq!(command.get_program(), tools.join("cl.exe"));
        let env = command
            .get_envs()
            .map(|(key, value)| (key.to_string_lossy().into_owned(), value.unwrap().to_string_lossy().into_owned()))
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(env["PATH"], tools.to_string_lossy());
        assert_eq!(env["LIB"], sdk.to_string_lossy());
    }

    #[test]
    fn missing_sdk_reports_named_component() {
        let temp = tempfile::tempdir().unwrap();
        for name in ["cl.exe", "link.exe", "lib.exe"] {
            std::fs::write(temp.path().join(name), "").unwrap();
        }
        let output = format!(
            "PATH={}\nINCLUDE={}\nLIB={}\n",
            temp.path().display(),
            temp.path().display(),
            temp.path().display()
        );
        let error = configure_from_dev_output(&mut Command::new("cl"), &output, None).unwrap_err();
        assert!(error.to_string().contains("Windows SDK"), "{error}");
    }

    #[test]
    fn llvm_linker_remains_available_when_vs_link_is_missing() {
        let temp = tempfile::tempdir().unwrap();
        let tools = temp.path().join("VS Tools");
        let llvm = temp.path().join("LLVM bin");
        let sdk = temp.path().join("SDK Libraries");
        for directory in [&tools, &llvm, &sdk] {
            std::fs::create_dir_all(directory).unwrap();
        }
        std::fs::write(tools.join("cl.exe"), "").unwrap();
        std::fs::write(llvm.join("lld-link.exe"), "").unwrap();
        std::fs::write(llvm.join("llvm-lib.exe"), "").unwrap();
        for name in ["kernel32.lib", "ucrt.lib", "msvcrt.lib"] {
            std::fs::write(sdk.join(name), "").unwrap();
        }
        let output = format!(
            "PATH={}\nINCLUDE={}\nLIB={}\nWindowsSdkDir={}\nWindowsSDKVersion=10.0.26100.0\\\n",
            tools.display(),
            tools.display(),
            sdk.display(),
            temp.path().display()
        );
        let mut command = Command::new("lld-link");
        configure_from_dev_output(&mut command, &output, Some(&llvm)).unwrap();
        assert_eq!(command.get_program(), llvm.join("lld-link.exe"));
        let mut librarian = Command::new("llvm-lib");
        configure_from_dev_output(&mut librarian, &output, Some(&llvm)).unwrap();
        assert_eq!(librarian.get_program(), llvm.join("llvm-lib.exe"));
    }

    #[test]
    fn complete_explicit_native_environment_wins() {
        let environment = [
            ("PATH".to_owned(), "C:\\custom\\bin".into()),
            ("INCLUDE".to_owned(), "C:\\custom\\include".into()),
            ("LIB".to_owned(), "C:\\custom\\lib".into()),
        ]
        .into_iter()
        .collect();
        assert!(complete_explicit_environment(&environment));
    }

    #[test]
    fn incomplete_newer_instance_falls_back_to_complete_build_tools() {
        let temp = tempfile::tempdir().unwrap();
        let tools = temp.path().join("Visual Studio Build Tools");
        let sdk = temp.path().join("Windows SDK Libraries");
        std::fs::create_dir_all(&tools).unwrap();
        std::fs::create_dir_all(&sdk).unwrap();
        for name in ["cl.exe", "link.exe", "lib.exe"] {
            std::fs::write(tools.join(name), "").unwrap();
        }
        for name in ["kernel32.lib", "ucrt.lib", "msvcrt.lib"] {
            std::fs::write(sdk.join(name), "").unwrap();
        }
        let incomplete = temp.path().join("Community");
        let complete = temp.path().join("BuildTools");
        let instances = [
            Instance { path: incomplete.clone(), complete: true, launchable: true, version: "17.15.0".into() },
            Instance { path: complete.clone(), complete: true, launchable: true, version: "17.14.0".into() },
        ];
        let full = format!(
            "PATH={}\nINCLUDE={}\nLIB={}\nWindowsSdkDir={}\nWindowsSDKVersion=10.0.26100.0\\\n",
            tools.display(),
            tools.display(),
            sdk.display(),
            temp.path().display()
        );
        let mut command = Command::new("cl");
        configure_from_instances(&mut command, &instances, None, |instance| {
            if instance == incomplete {
                Ok("PATH=C:\\incomplete\nINCLUDE=C:\\incomplete\nLIB=C:\\incomplete\n".into())
            } else if instance == complete {
                Ok(full.clone())
            } else {
                panic!("unexpected instance: {}", instance.display())
            }
        })
        .unwrap();
        assert_eq!(command.get_program(), tools.join("cl.exe"));
    }
}
