// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

//! Shell completion: the scripts `ketch completions` prints and `self install`
//! writes, and the package names those scripts ask for while completing.
//!
//! clap_complete writes the static part — commands, aliases, flags — from
//! `Cli::command()`. Package names are not known when a script is generated,
//! so the script calls back into `ketch __complete <kind>`, which prints the
//! names one per line. That command is shell-agnostic on purpose: the bash and
//! PowerShell scripts both call it, with the same table of commands.
//!
//! Why not clap_complete's `CompleteEnv`: it lives behind the
//! `unstable-dynamic` feature, outside clap_complete's semver promise, and its
//! docs warn that the protocol between the registered shell code and the binary
//! may change between releases. ketch writes its script to disk at
//! `self install`, so a patch update of clap_complete could leave every
//! installed script talking a protocol the binary no longer speaks.
//! `__complete` is ours, so its contract changes only when we change it.
//!
//! `__complete` is parsed here, before clap sees the command line, rather than
//! declared as a hidden subcommand: clap_complete's bash generator lists hidden
//! subcommands too, and `ketch <TAB>` must not offer an internal one.

use crate::cli::Cli;
use crate::config::{self, Config};
use crate::error::Error;
use crate::{registry, state::State, ui};
use clap::{ArgAction, CommandFactory, Parser, ValueEnum};
use std::ffi::OsString;
use std::path::PathBuf;

/// The argument that routes a run to [`run`] instead of the clap surface.
pub const COMMAND: &str = "__complete";

/// Which names a completion asks for.
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Packages recorded in the state file.
    Installed,
    /// Packages in the local registry copy. Never fetched: completion runs on
    /// every <TAB> and must not wait on the network.
    Registry,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Installed => "installed",
            Self::Registry => "registry",
        }
    }
}

/// Subcommands whose positional argument is a package name, and which names
/// fit it. `why` and `info` also take names that are not installed, but the
/// installed ones are what a user reaches for most.
const DYNAMIC: &[(&str, Kind)] = &[
    ("uninstall", Kind::Installed),
    ("upgrade", Kind::Installed),
    ("pin", Kind::Installed),
    ("unpin", Kind::Installed),
    ("link", Kind::Installed),
    ("unlink", Kind::Installed),
    ("info", Kind::Installed),
    ("why", Kind::Installed),
    ("changelog", Kind::Installed),
    ("rollback", Kind::Installed),
    ("install", Kind::Registry),
    ("search", Kind::Registry),
];

/// `ketch __complete [--root DIR] <KIND> [PREFIX]`.
#[derive(Parser, Debug)]
#[command(name = COMMAND, disable_help_flag = true, disable_version_flag = true)]
struct Request {
    /// The ketch root the completed command line names with `--root`.
    #[arg(long, value_name = "DIR")]
    root: Option<PathBuf>,
    #[arg(value_enum)]
    kind: Kind,
    /// Only names starting with this; everything when absent.
    #[arg(allow_hyphen_values = true)]
    prefix: Option<String>,
}

/// Handle `ketch __complete …` when that is what `args` (the full command
/// line, program name first) asks for. Returns the exit code, or `None` when
/// this is an ordinary run for clap to parse.
pub fn intercept(args: &[OsString]) -> Option<i32> {
    if args.get(1).is_none_or(|a| a != COMMAND) {
        return None;
    }
    let request = match Request::try_parse_from(&args[1..]) {
        Ok(request) => request,
        Err(e) => {
            // clap's message carries its own `error: ` headline; ui adds one.
            let text = e.to_string();
            ui::error(&Error::msg(text.trim_start_matches("error: ").trim_end()));
            return Some(2);
        }
    };
    // A completion that fails prints nothing: the shell falls back to its
    // default, and an error message in the middle of a command line helps no one.
    let Ok(cfg) = Config::load(request.root, crate::ui::report()) else {
        return Some(1);
    };
    for name in candidates(&cfg, request.kind, request.prefix.as_deref().unwrap_or("")) {
        ui::out(&name);
    }
    Some(0)
}

/// Names of `kind` starting with `prefix`, sorted and without duplicates.
///
/// Registry folder names are written by someone else, and the bash script puts
/// each one straight into the command line being edited, so only plain names
/// pass: anything a shell would treat as syntax is dropped, not quoted.
pub fn candidates(cfg: &Config, kind: Kind, prefix: &str) -> Vec<String> {
    let mut names: Vec<String> = match kind {
        Kind::Installed => State::load(cfg)
            .map(|state| state.names().into_iter().map(str::to_string).collect())
            .unwrap_or_default(),
        Kind::Registry => registry::load(&crate::ui::ctx(cfg))
            .into_iter()
            .map(|(manifest, _)| manifest.name)
            .collect(),
    };
    names.retain(|name| name.starts_with(prefix) && is_plain(name));
    names.sort();
    names.dedup();
    names
}

fn is_plain(name: &str) -> bool {
    config::sanitize_component(name) == name
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '+' | '@'))
}

/// The completion script for `shell`, as `ketch completions` prints it.
pub fn script(shell: clap_complete::Shell) -> Vec<u8> {
    let mut command = Cli::command();
    let name = command.get_name().to_string();
    let mut buf = Vec::new();
    clap_complete::generate(shell, &mut command, &name, &mut buf);
    match shell {
        clap_complete::Shell::Bash => buf.extend_from_slice(bash_packages(&name).as_bytes()),
        clap_complete::Shell::PowerShell => {
            buf = powershell_with_packages(&String::from_utf8_lossy(&buf)).into_bytes();
        }
        _ => {}
    }
    buf
}

/// The line clap's PowerShell completer opens with. The package lookup goes
/// straight after it, inside clap's own script block, so there is one
/// completer to register and no closure to keep alive.
const POWERSHELL_PARAM: &str = "param($wordToComplete, $commandAst, $cursorPosition)\n";

/// Package-name completion spliced into clap's PowerShell completer, the
/// same rules as [`bash_packages`]: when the word being completed is a
/// package for a command in [`DYNAMIC`], answer from `ketch __complete`;
/// otherwise fall through to clap's static answers below it.
///
/// Written for Windows PowerShell 5.1 as well as 7: no `??`, no ternary.
/// Comparisons are the case-sensitive `-ceq`/`-ccontains`, because clap's
/// subcommands and flags are. When clap stops emitting [`POWERSHELL_PARAM`]
/// the script is left as clap wrote it, and a test says so.
fn powershell_with_packages(clap_script: &str) -> String {
    let Some(at) = clap_script.find(POWERSHELL_PARAM) else {
        return clap_script.to_string();
    };
    let mut command = Cli::command();
    command.build();
    let mut cases = String::new();
    for (sub_name, kind) in DYNAMIC {
        let Some(sub) = command.find_subcommand(sub_name) else {
            continue;
        };
        let mut names = vec![sub.get_name()];
        names.extend(sub.get_visible_aliases());
        let multi = sub
            .get_positionals()
            .any(|arg| matches!(arg.get_action(), ArgAction::Append));
        cases.push_str(&format!(
            "                {}if (@({}) -ccontains $sub) {{ $kind = '{}'; $multi = ${}; $takes += @({}) }}\n",
            if cases.is_empty() { "" } else { "else" },
            powershell_list(&names),
            kind.as_str(),
            multi,
            powershell_list(&value_options(sub)),
        ));
    }
    cases.push_str("                else { break }\n");
    let block = POWERSHELL_PACKAGES
        .replace("@GLOBAL@", &powershell_list(&value_options(&command)))
        .replace("@CASES@", &cases)
        .replace("@COMMAND@", COMMAND);
    let split = at + POWERSHELL_PARAM.len();
    format!("{}{block}{}", &clap_script[..split], &clap_script[split..])
}

/// `'a','b'` — every item is a clap name or flag, so none holds a quote.
fn powershell_list<S: AsRef<str>>(items: &[S]) -> String {
    items
        .iter()
        .map(|s| format!("'{}'", s.as_ref().replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(",")
}

const POWERSHELL_PACKAGES: &str = r#"
    # Package names for the commands that take them, from `ketch @COMMAND@`.
    # Any other position falls through to clap's completions below.
    $packages = & {
        $takes = @(@GLOBAL@)
        $words = @($commandAst.CommandElements | Where-Object { $_.Extent.EndOffset -lt $cursorPosition })
        $texts = @($words | ForEach-Object { if ($_ -is [StringConstantExpressionAst]) { $_.Value } else { $_.Extent.Text } })
        $sub = $null; $kind = $null; $multi = $false; $root = $null; $seen = 0
        for ($i = 1; $i -lt $texts.Count; $i++) {
            $word = [string]$texts[$i]
            if ($word -ceq '--root') {
                if ($i + 1 -lt $texts.Count) { $root = [string]$texts[$i + 1] }
                $i++
                continue
            }
            if ($word.StartsWith('--root=')) { $root = $word.Substring(7); continue }
            if ($null -eq $sub) {
                if ($takes -ccontains $word) { $i++; continue }
                if ($word.StartsWith('-')) { continue }
                $sub = $word
@CASES@                continue
            }
            if ($takes -ccontains $word) { $i++ }
            elseif (-not $word.StartsWith('-')) { $seen++ }
        }
        if ($null -eq $kind) { return }
        if ($wordToComplete.StartsWith('-') -or ($takes -ccontains [string]$texts[$texts.Count - 1])) { return }
        if (-not $multi -and $seen -gt 0) { return }
        $request = @('@COMMAND@')
        if ($root) { $request += @('--root', $root) }
        $request += @($kind, '--', $wordToComplete)
        & $texts[0] @request 2>$null | Where-Object { $_ } | ForEach-Object {
            [CompletionResult]::new($_, $_, [CompletionResultType]::ParameterValue, $_)
        }
    }
    if ($packages) { return $packages }
"#;

/// A bash function that completes package names for the subcommands in
/// [`DYNAMIC`] and hands every other case to clap's `_<name>`, then registers
/// itself in clap's place.
///
/// The value-taking options are read from the clap tree, so an option added
/// later is skipped over correctly without touching this function. The script
/// stays bash 3.2 compatible — macOS still ships that as `/bin/bash`.
fn bash_packages(name: &str) -> String {
    let mut command = Cli::command();
    command.build();
    let global = value_options(&command);
    let mut cases = String::new();
    for (sub_name, kind) in DYNAMIC {
        let Some(sub) = command.find_subcommand(sub_name) else {
            continue;
        };
        let mut names = vec![sub.get_name()];
        names.extend(sub.get_visible_aliases());
        let multi = sub
            .get_positionals()
            .any(|arg| matches!(arg.get_action(), ArgAction::Append));
        cases.push_str(&format!(
            "                {}) kind={}; multi={}; takes=\" {} \" ;;\n",
            names.join("|"),
            kind.as_str(),
            if multi { "1" } else { "" },
            value_options(sub).join(" "),
        ));
    }
    let function = format!("_{name}_packages");
    format!(
        r#"
# Package names for the commands that take them, from `{name} {COMMAND}`.
# Everything else goes to clap's _{name} above.
{function}() {{
    local cur="${{COMP_WORDS[COMP_CWORD]}}" prev="${{COMP_WORDS[COMP_CWORD-1]}}"
    local bin="${{COMP_WORDS[0]}}" i word sub="" kind="" multi="" root="" line
    local takes=" {global} "
    local seen=0
    for (( i = 1; i < COMP_CWORD; i++ )); do
        word="${{COMP_WORDS[i]}}"
        case "$word" in
            --root) root="${{COMP_WORDS[i+1]}}"; (( i++ )); continue ;;
            --root=*) root="${{word#--root=}}"; continue ;;
        esac
        if [[ -z "$sub" ]]; then
            [[ "$word" == -* ]] && continue
            sub="$word"
            case "$sub" in
{cases}                *) break ;;
            esac
            continue
        fi
        if [[ "$takes" == *" $word "* ]]; then
            (( i++ ))
        elif [[ "$word" != -* ]]; then
            (( seen++ ))
        fi
    done
    if [[ -z "$kind" || "$cur" == -* || "$takes" == *" $prev "* ]] || [[ -z "$multi" && $seen -gt 0 ]]; then
        _{name} "$@"
        return
    fi
    [[ "$bin" == "~/"* ]] && bin="$HOME/${{bin#\~/}}"
    [[ "$root" == "~/"* ]] && root="$HOME/${{root#\~/}}"
    local args=({COMMAND})
    [[ -n "$root" ]] && args+=(--root "$root")
    args+=("$kind" -- "$cur")
    COMPREPLY=()
    while IFS= read -r line; do
        [[ -n "$line" ]] && COMPREPLY+=("$line")
    done < <("$bin" "${{args[@]}}" 2>/dev/null)
}}

if [[ "${{BASH_VERSINFO[0]}}" -eq 4 && "${{BASH_VERSINFO[1]}}" -ge 4 || "${{BASH_VERSINFO[0]}}" -gt 4 ]]; then
    complete -F {function} -o nosort -o bashdefault -o default {name}
else
    complete -F {function} -o bashdefault -o default {name}
fi
"#,
        global = global.join(" "),
    )
}

/// Every spelling of the options on `command` that take a value.
fn value_options(command: &clap::Command) -> Vec<String> {
    let mut out = Vec::new();
    for arg in command.get_arguments() {
        if arg.is_positional() || !arg.get_action().takes_values() {
            continue;
        }
        if let Some(longs) = arg.get_long_and_visible_aliases() {
            out.extend(longs.into_iter().map(|l| format!("--{l}")));
        }
        if let Some(shorts) = arg.get_short_and_visible_aliases() {
            out.extend(shorts.into_iter().map(|s| format!("-{s}")));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::collections::BTreeSet;

    fn words(script: &str) -> BTreeSet<&str> {
        script
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
            .filter(|w| !w.is_empty())
            .collect()
    }

    fn walk(command: &clap::Command, script: &BTreeSet<&str>, missing: &mut Vec<String>) {
        for arg in command.get_arguments() {
            if arg.is_hide_set() || arg.is_positional() {
                continue;
            }
            for long in arg.get_long_and_visible_aliases().unwrap_or_default() {
                let flag = format!("--{long}");
                if !script.contains(flag.as_str()) {
                    missing.push(format!("{} {flag}", command.get_name()));
                }
            }
            for short in arg.get_short_and_visible_aliases().unwrap_or_default() {
                let flag = format!("-{short}");
                if !script.contains(flag.as_str()) {
                    missing.push(format!("{} {flag}", command.get_name()));
                }
            }
        }
        for sub in command.get_subcommands() {
            if sub.is_hide_set() {
                continue;
            }
            for name in std::iter::once(sub.get_name()).chain(sub.get_visible_aliases()) {
                if !script.contains(name) {
                    missing.push(format!("{} {name}", command.get_name()));
                }
            }
            walk(sub, script, missing);
        }
    }

    #[test]
    fn the_bash_script_names_every_visible_subcommand_alias_and_flag() {
        let script = String::from_utf8(script(clap_complete::Shell::Bash)).expect("utf-8");
        let mut command = Cli::command();
        command.build();
        let mut missing = Vec::new();
        walk(&command, &words(&script), &mut missing);
        assert_eq!(missing, Vec::<String>::new());
    }

    #[test]
    fn every_dynamic_subcommand_exists_and_takes_one_positional() {
        let command = Cli::command();
        for (name, _) in DYNAMIC {
            let sub = command
                .find_subcommand(name)
                .unwrap_or_else(|| panic!("no subcommand `{name}`"));
            assert_eq!(sub.get_positionals().count(), 1, "{name}");
        }
    }

    #[test]
    fn the_bash_script_registers_the_package_completer_last() {
        let script = String::from_utf8(script(clap_complete::Shell::Bash)).expect("utf-8");
        let last = script
            .lines()
            .rfind(|l| l.trim_start().starts_with("complete -F"))
            .expect("a registration");
        assert!(last.contains("-F _ketch_packages "), "{last}");
        assert!(script.contains("uninstall|remove|rm) kind=installed; multi=1;"));
        assert!(script.contains("rollback) kind=installed; multi=;"));
        assert!(script.contains("install|i) kind=registry; multi=1; takes=\" --path "));
    }

    #[test]
    fn the_script_self_install_writes_is_the_one_completions_prints() {
        let tmp = tempfile::tempdir().expect("temp dir");
        ketch_core::extra::write_ketch_docs(tmp.path(), crate::self_docs::SELF_DOCS)
            .expect("write docs");
        let written =
            std::fs::read(tmp.path().join("share/ketch/completions/ketch")).expect("bash script");
        assert_eq!(written, script(clap_complete::Shell::Bash));
    }

    #[test]
    fn the_powershell_script_asks_for_packages_inside_clap_completer() {
        let script = String::from_utf8(script(clap_complete::Shell::PowerShell)).expect("utf-8");
        let param = script.find(POWERSHELL_PARAM).expect("clap's param line");
        let lookup = script.find("$packages = & {").expect("the package lookup");
        let clap_body = script.find("$commandElements = ").expect("clap's body");
        assert!(param < lookup && lookup < clap_body, "{script}");
        assert_eq!(script.matches("Register-ArgumentCompleter").count(), 1);
        assert!(script.contains(
            "if (@('uninstall','remove','rm') -ccontains $sub) { $kind = 'installed'; $multi = $true; "
        ));
        assert!(script.contains(
            "elseif (@('rollback') -ccontains $sub) { $kind = 'installed'; $multi = $false; "
        ));
        assert!(script.contains("elseif (@('install','i') -ccontains $sub) { $kind = 'registry'; "));
        assert!(script.contains("$request = @('__complete')"));
        for placeholder in ["@GLOBAL@", "@CASES@", "@COMMAND@"] {
            assert!(!script.contains(placeholder), "{placeholder} left unfilled");
        }
    }

    #[test]
    fn the_powershell_script_names_every_visible_subcommand_alias_and_flag() {
        let script = String::from_utf8(script(clap_complete::Shell::PowerShell)).expect("utf-8");
        let mut command = Cli::command();
        command.build();
        let mut missing = Vec::new();
        walk(&command, &words(&script), &mut missing);
        assert_eq!(missing, Vec::<String>::new());
    }

    #[test]
    fn a_clap_script_without_the_param_line_is_left_alone() {
        assert_eq!(powershell_with_packages("# other\n"), "# other\n");
    }

    #[test]
    fn other_shells_get_clap_script_unchanged() {
        let script = String::from_utf8(script(clap_complete::Shell::Zsh)).expect("utf-8");
        assert!(!script.contains(COMMAND));
    }

    #[test]
    fn plain_names_pass_and_shell_syntax_does_not() {
        for good in ["ripgrep", "fd-find", "gh_cli", "node@22", "c++", "a.b"] {
            assert!(is_plain(good), "{good}");
        }
        for bad in ["$(rm)", "a b", "a;b", "`x`", "-flag", "a/b", "é", ""] {
            assert!(!is_plain(bad), "{bad}");
        }
    }

    #[test]
    fn candidates_are_filtered_by_prefix_sorted_and_plain() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let cfg =
            Config::load(Some(tmp.path().to_path_buf()), crate::ui::report()).expect("config");
        for name in ["ripgrep", "ripcord", "fd", "$(evil)"] {
            let dir = cfg.registry_dir.join(name);
            std::fs::create_dir_all(&dir).expect("dir");
            std::fs::write(
                dir.join(registry::PACKAGE_FILE),
                format!("name = \"{name}\"\nsource = \"test:{name}\"\n"),
            )
            .expect("write");
        }
        assert_eq!(
            candidates(&cfg, Kind::Registry, "rip"),
            vec!["ripcord".to_string(), "ripgrep".to_string()]
        );
        assert_eq!(candidates(&cfg, Kind::Registry, "").len(), 3);
        assert_eq!(candidates(&cfg, Kind::Installed, ""), Vec::<String>::new());
    }

    #[test]
    fn intercept_ignores_ordinary_command_lines() {
        let args: Vec<OsString> = ["ketch", "install", "__complete"]
            .map(OsString::from)
            .to_vec();
        assert_eq!(intercept(&args), None);
    }
}
