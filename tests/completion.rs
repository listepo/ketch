//! Bash completion, driven through a real bash: the script `ketch completions
//! bash` prints is evaluated, and the function it registers for `ketch` is
//! called the way bash calls it on <TAB>.
//!
//! Runs with whatever `bash` is on PATH — on macOS that is `/bin/bash` 3.2, so
//! this also proves the script needs nothing newer. Offline: package names come
//! from a local install and a registry folder written into the sandbox.
#![cfg(unix)]

mod support;

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use support::Sandbox;

/// Complete `line` (words split on spaces; a trailing space means an empty
/// current word) and return the candidates bash would offer.
fn complete(sandbox: &Sandbox, line: &str) -> Vec<String> {
    let exe = Path::new(env!("CARGO_BIN_EXE_ketch"));
    let dir = exe.parent().expect("binary has a parent directory");
    let mut words: Vec<&str> = line.split(' ').collect();
    if words.last() == Some(&"") {
        words.pop();
        words.push("''");
    }
    let script = format!(
        r#"eval "$(ketch completions bash)"
fn=$(complete -p ketch | sed -n 's/.*-F \([^ ]*\).*/\1/p')
COMP_WORDS=({words})
COMP_CWORD=$(( ${{#COMP_WORDS[@]}} - 1 ))
COMP_LINE={line:?}
COMP_POINT=${{#COMP_LINE}}
"$fn" ketch "${{COMP_WORDS[COMP_CWORD]}}" "${{COMP_WORDS[COMP_CWORD-1]}}"
printf '%s\n' "${{COMPREPLY[@]}}"
"#,
        words = words.join(" "),
    );
    let out = sandbox.ketch_from(Path::new("bash"), &["-c", &script], dir);
    assert!(
        out.status.success(),
        "bash failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut found: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    found.sort();
    found
}

fn install_local(sandbox: &Sandbox, name: &str) {
    let fixture = sandbox.fixture(name);
    std::fs::write(&fixture, format!("#!/bin/sh\necho {name}\n")).expect("write program");
    std::fs::set_permissions(&fixture, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    sandbox.ok(&["install", "--path", fixture.to_str().expect("utf-8"), "-y"]);
}

#[test]
fn subcommands_complete_from_their_prefix() {
    let sandbox = Sandbox::new();
    assert_eq!(
        complete(&sandbox, "ketch un"),
        ["uninstall", "unlink", "unpin"]
    );
}

#[test]
fn uninstall_offers_installed_packages() {
    let sandbox = Sandbox::new();
    install_local(&sandbox, "ripgrep");
    install_local(&sandbox, "fd");
    assert_eq!(complete(&sandbox, "ketch uninstall r"), ["ripgrep"]);
    assert_eq!(complete(&sandbox, "ketch rm "), ["fd", "ripgrep"]);
    // Several names are allowed, so the second one completes too.
    assert_eq!(complete(&sandbox, "ketch pin fd r"), ["ripgrep"]);
}

#[test]
fn a_command_that_takes_one_package_stops_after_it() {
    let sandbox = Sandbox::new();
    install_local(&sandbox, "ripgrep");
    assert_eq!(complete(&sandbox, "ketch info r"), ["ripgrep"]);
    assert!(!complete(&sandbox, "ketch info ripgrep r").contains(&"ripgrep".to_string()));
}

#[test]
fn an_option_value_is_not_completed_as_a_package() {
    let sandbox = Sandbox::new();
    install_local(&sandbox, "ripgrep");
    assert!(!complete(&sandbox, "ketch upgrade --bin r").contains(&"ripgrep".to_string()));
    assert_eq!(complete(&sandbox, "ketch upgrade --bin x r"), ["ripgrep"]);
}

#[test]
fn install_offers_registry_packages() {
    let sandbox = Sandbox::new();
    sandbox.registry_package("ripcord", "name = \"ripcord\"\nsource = \"test:ripcord\"\n");
    install_local(&sandbox, "ripgrep");
    assert_eq!(complete(&sandbox, "ketch install ri"), ["ripcord"]);
    assert_eq!(complete(&sandbox, "ketch i ri"), ["ripcord"]);
}

#[test]
fn a_root_on_the_command_line_is_the_one_asked() {
    let sandbox = Sandbox::new();
    let other = Sandbox::new();
    install_local(&other, "zoxide");
    let line = format!("ketch --root {} uninstall z", other.root().display());
    assert_eq!(complete(&sandbox, &line), ["zoxide"]);
    assert_eq!(
        complete(&sandbox, "ketch uninstall z"),
        Vec::<String>::new()
    );
}
