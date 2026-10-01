//! Windows completion, driven through the real shells: PowerShell's own
//! `TabExpansion2` against the script `ketch completions powershell` prints,
//! and cmd's AutoRun and the PowerShell profiles that `completions --install`
//! writes and `self uninstall` takes back.
//!
//! The second half touches the real `HKCU\Software\Microsoft\Command
//! Processor\AutoRun` and the real profiles under Documents: neither can be
//! pointed at a sandbox. A guard saves both first and puts them back on drop,
//! whether the assertions pass or not.
#![cfg(target_os = "windows")]

mod support;

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use support::Sandbox;

/// The directory holding the built `ketch.exe`, put first on PATH so the
/// completer's call back into `ketch __complete` reaches this build.
fn exe_dir() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_ketch"))
        .parent()
        .expect("binary has a parent directory")
}

/// Run a PowerShell command with the sandbox environment and return stdout.
fn powershell(sandbox: &Sandbox, shell: &str, command: &str) -> String {
    let out = sandbox.ketch_from(
        Path::new(shell),
        &["-NoProfile", "-NonInteractive", "-Command", command],
        exe_dir(),
    );
    assert!(
        out.status.success(),
        "{shell} failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// What `shell` offers on <TAB> at the end of `line`.
fn complete(sandbox: &Sandbox, shell: &str, line: &str) -> Vec<String> {
    let script = sandbox.fixture("ketch-completion.ps1");
    std::fs::write(&script, sandbox.ok(&["completions", "powershell"])).expect("write script");
    let command = format!(
        ". '{}'; (TabExpansion2 -inputScript '{line}' -cursorColumn {}).CompletionMatches | ForEach-Object {{ $_.CompletionText }}",
        script.display(),
        line.len()
    );
    let mut found: Vec<String> = powershell(sandbox, shell, &command)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    found.sort();
    found
}

#[test]
fn both_powershells_complete_subcommands_and_registry_packages() {
    let sandbox = Sandbox::new();
    sandbox.registry_package("ripcord", "name = \"ripcord\"\nsource = \"test:ripcord\"\n");
    for shell in ["pwsh", "powershell"] {
        let subs = complete(&sandbox, shell, "ketch ins");
        assert!(subs.contains(&"install".to_string()), "{shell}: {subs:?}");
        assert_eq!(
            complete(&sandbox, shell, "ketch install ri"),
            ["ripcord"],
            "{shell}"
        );
    }
}

/// cmd's AutoRun: kind and raw value, or `None` when unset.
fn read_autorun(sandbox: &Sandbox) -> Option<(String, String)> {
    let out = powershell(
        sandbox,
        "powershell",
        "$k = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Software\\Microsoft\\Command Processor'); \
         if ($k -and ($k.GetValueNames() -contains 'AutoRun')) { \
         [Console]::Out.Write($k.GetValueKind('AutoRun').ToString() + [char]10 + \
         [string]$k.GetValue('AutoRun', '', 'DoNotExpandEnvironmentNames')) }",
    );
    out.split_once('\n')
        .map(|(kind, value)| (kind.trim().to_string(), value.to_string()))
}

fn write_autorun(value: Option<&(String, String)>) {
    let script = match value {
        Some(_) => {
            "$k = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Software\\Microsoft\\Command Processor'); \
             $k.SetValue('AutoRun', $env:TEST_AUTORUN_VALUE, [Microsoft.Win32.RegistryValueKind]$env:TEST_AUTORUN_KIND); $k.Close()"
        }
        None => {
            "$k = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Software\\Microsoft\\Command Processor', $true); \
             if ($k) { $k.DeleteValue('AutoRun', $false); $k.Close() }"
        }
    };
    let (kind, text) = value.cloned().unwrap_or_default();
    let status = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("TEST_AUTORUN_VALUE", text)
        .env("TEST_AUTORUN_KIND", kind)
        .status()
        .expect("run powershell");
    assert!(status.success(), "could not write AutoRun");
}

/// Puts AutoRun and both profiles back the way they were before the test.
struct Restore {
    autorun: Option<(String, String)>,
    profiles: Vec<(PathBuf, Option<Vec<u8>>)>,
}

impl Drop for Restore {
    fn drop(&mut self) {
        write_autorun(self.autorun.as_ref());
        for (file, bytes) in &self.profiles {
            match bytes {
                Some(bytes) => {
                    let _ = std::fs::write(file, bytes);
                }
                None => {
                    let _ = std::fs::remove_file(file);
                }
            }
        }
    }
}

#[test]
fn install_adds_the_doskey_line_and_profile_blocks_and_self_uninstall_restores_them() {
    let sandbox = Sandbox::new();
    let documents = PathBuf::from(
        powershell(
            &sandbox,
            "powershell",
            "[Console]::Out.Write([Environment]::GetFolderPath('MyDocuments', 'DoNotVerify'))",
        )
        .trim(),
    );
    let profiles: Vec<PathBuf> = ["PowerShell", "WindowsPowerShell"]
        .iter()
        .map(|dir| documents.join(dir).join("profile.ps1"))
        .collect();
    let _restore = Restore {
        autorun: read_autorun(&sandbox),
        profiles: profiles
            .iter()
            .map(|p| (p.clone(), std::fs::read(p).ok()))
            .collect(),
    };
    let before: Vec<Option<Vec<u8>>> = profiles.iter().map(|p| std::fs::read(p).ok()).collect();

    let earlier = ("String".to_string(), "set KETCH_TEST_AUTORUN=1".to_string());
    write_autorun(Some(&earlier));

    // A package named ketch is what `completions --install` places docs for.
    let exe = sandbox.fixture("ketch.exe");
    std::fs::copy(env!("CARGO_BIN_EXE_ketch"), &exe).expect("copy ketch");
    sandbox.ok(&[
        "install",
        "--path",
        exe.to_str().expect("utf-8"),
        "--name",
        "ketch",
        "-y",
    ]);
    let out = sandbox.ketch(&["completions", "powershell", "--install"]);
    let installed = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "{installed}");

    let doskey = sandbox
        .root()
        .join("share")
        .join("ketch")
        .join("ketch.doskey");
    let macros = std::fs::read_to_string(&doskey).expect("the macro file");
    assert!(macros.contains("ki=ketch install $*"), "{macros}");
    let (kind, value) = read_autorun(&sandbox).expect(&installed);
    assert_eq!(kind, "String");
    // The root may be spelled differently in the value (a short 8.3 name),
    // so the ends are checked rather than the whole path.
    assert!(
        value.starts_with("set KETCH_TEST_AUTORUN=1 & doskey /macrofile=\"")
            && value.ends_with("\\share\\ketch\\ketch.doskey\""),
        "{value}"
    );

    // A new cmd runs AutoRun, so the macros are there without anything else.
    // doskey keeps macros per console, and a test has none of its own: the
    // cmd gets a hidden one.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let listed = std::process::Command::new("cmd")
        .args(["/c", "doskey /macros & echo autorun=%KETCH_TEST_AUTORUN%"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .expect("run cmd");
    let listed = format!(
        "{}{}",
        String::from_utf8_lossy(&listed.stdout),
        String::from_utf8_lossy(&listed.stderr)
    );
    assert!(
        listed.contains("autorun=1"),
        "AutoRun did not run:\n{listed}"
    );
    assert!(listed.contains("ki=ketch install $*"), "{listed}");

    let policy = powershell(&sandbox, "powershell", "Get-ExecutionPolicy");
    let runs_scripts = ["Unrestricted", "RemoteSigned", "Bypass"].contains(&policy.trim());
    let desktop = std::fs::read_to_string(&profiles[1]).unwrap_or_default();
    if runs_scripts || before[1].is_some() {
        assert!(
            desktop.contains("# >>> ketch >>>") && desktop.contains("ketch.ps1"),
            "{} holds:\n{desktop}\n--- completions --install said ---\n{installed}",
            profiles[1].display()
        );
    }

    sandbox.ok(&["self", "uninstall", "--yes"]);

    assert_eq!(read_autorun(&sandbox), Some(earlier));
    assert!(!doskey.exists(), "the macro file survived");
    for (file, was) in profiles.iter().zip(&before) {
        assert_eq!(
            &std::fs::read(file).ok(),
            was,
            "{} is not what it was",
            file.display()
        );
    }
}
