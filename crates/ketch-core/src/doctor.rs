// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-only
// Licensed under GPL-3.0 only; see https://www.gnu.org/licenses/gpl-3.0.html

//! What `ketch doctor` checks: the machine, the install tree and the files
//! ketch keeps, as a list of [`DoctorCheck`]s.
//!
//! The checks used to live in the binary's command body. They read the install
//! tree and nothing else, so they belong with the rest of the code that does;
//! here a graphical front end gets the same list the CLI prints, and the
//! command keeps only `--fix` and the rendering.

use crate::config::Config;
use crate::platform::{self, CheckStatus, DoctorCheck};
use crate::registry;
use crate::report::Ctx;
use crate::self_update;
use crate::shell;
use crate::state::State;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Every check, in the order `ketch doctor` prints them.
///
/// Never fails: a check that cannot run is itself a failed check, because the
/// report exists to finish even when part of the machine is broken.
pub fn checks(cx: &Ctx<'_>) -> Vec<DoctorCheck> {
    let cfg = cx.cfg;
    let mut checks = vec![DoctorCheck::ok(
        "version",
        format!(
            "ketch {} for {}",
            self_update::display_version(),
            cfg.target
        ),
    )];

    // Not a platform check: every shell reads the same startup files wherever
    // it runs, so a second platform would only duplicate this.
    checks.push(shell::path_check(cfg));
    if let Some(check) = shell::stale_registry_check(cfg) {
        checks.push(check);
    }
    if let Some(check) = path_binary_check(cfg) {
        checks.push(check);
    }
    if let Some(check) = self_update::stale_aside_check(cfg) {
        checks.push(check);
    }

    match platform::host() {
        Ok(host) => {
            checks.extend(platform::extra_destination_checks(host.as_ref()));
            checks.extend(host.doctor(cfg));
        }
        Err(e) => checks.push(DoctorCheck::fail(
            "platform",
            e.to_string(),
            "This build of ketch does not support this operating system.",
        )),
    }
    checks.push(log_check(cfg));
    checks.push(registry_check(cx));
    checks.extend(store_checks(cfg));
    if let Some(check) = lock_check(cfg) {
        checks.push(check);
    }
    checks
}

/// How many checks failed outright; the command exits non-zero when any did.
pub fn failed(checks: &[DoctorCheck]) -> usize {
    checks
        .iter()
        .filter(|c| c.status == CheckStatus::Fail)
        .count()
}

/// Where this machine's log is, so nobody has to be told twice.
fn log_check(cfg: &Config) -> DoctorCheck {
    if cfg.log_level == crate::log::Level::Off {
        return DoctorCheck::ok("log", "off".to_string());
    }
    let size = std::fs::metadata(&cfg.log_file)
        .map(|m| format!(" · {}", crate::text::bytes(m.len())))
        .unwrap_or_default();
    DoctorCheck::ok(
        "log",
        format!(
            "{} ({}, {}){size}",
            cfg.log_file.display(),
            cfg.log_level,
            cfg.log_format
        ),
    )
}

fn registry_check(cx: &Ctx<'_>) -> DoctorCheck {
    let cfg = cx.cfg;
    if !registry::exists(cfg) {
        return DoctorCheck::warn(
            "registry",
            format!("no local copy of {}", cfg.registry),
            "Run `ketch update`.",
        );
    }
    let count = registry::load(cx).len();
    match registry::load_meta(cfg) {
        Ok(Some(meta)) => {
            let age = registry::age_phrase(meta.fetched_at);
            let source = meta
                .revision
                .as_deref()
                .or(meta.etag.as_deref())
                .unwrap_or("unknown revision");
            DoctorCheck::ok(
                "registry",
                format!("{count} packages from {} · {age} · {source}", cfg.registry),
            )
        }
        Ok(None) => DoctorCheck::warn(
            "registry",
            format!("{count} packages from {}; fetch time unknown", cfg.registry),
            "Run `ketch update`.",
        ),
        Err(e) => DoctorCheck::warn(
            "registry",
            format!("{count} packages from {}; {e}", cfg.registry),
            "Run `ketch update`.",
        ),
    }
}

/// Everything ketch itself owns: the store matches the state file, and every
/// link still points at the package that claims it.
fn store_checks(cfg: &Config) -> Vec<DoctorCheck> {
    let state = match State::load(cfg) {
        Ok(s) => s,
        Err(e) => {
            return vec![DoctorCheck::fail(
                "state",
                e.to_string(),
                format!("Inspect or remove {}.", cfg.state_file.display()),
            )]
        }
    };

    let mut checks = Vec::new();
    let mut missing_payloads = Vec::new();
    let mut broken_links = Vec::new();
    for pkg in state.iter() {
        if !pkg.prefix.exists() {
            missing_payloads.push(pkg.name.clone());
            // Its links cannot be sound either; one message per package is enough.
            continue;
        }
        for link in &pkg.links {
            // `exists` follows symlinks, so this catches both a deleted link and
            // one left dangling by a manual removal inside the store.
            if !link.link.exists() {
                broken_links.push(format!("{} -> {}", pkg.name, link.link.display()));
            }
        }
    }

    checks.push(match missing_payloads.len() {
        0 => DoctorCheck::ok("packages", format!("{} installed", state.iter().count())),
        n => DoctorCheck::fail(
            "packages",
            format!(
                "{n} packages have no files: {}",
                missing_payloads.join(", ")
            ),
            format!(
                "Run `ketch install --force {}`.",
                missing_payloads.join(" ")
            ),
        ),
    });

    if !broken_links.is_empty() {
        checks.push(DoctorCheck::warn(
            "links",
            format!(
                "{} broken links: {}",
                broken_links.len(),
                broken_links.join(", ")
            ),
            "Run `ketch link <pkg>` to recreate them.",
        ));
    }

    let known: BTreeSet<String> = state.iter().map(|pkg| pkg.name.clone()).collect();
    let orphans = orphan_store_dirs(&cfg.store_dir, &known);
    if !orphans.is_empty() {
        checks.push(DoctorCheck::warn(
            "orphans",
            format!(
                "{} prefixes have no state entry: {}",
                orphans.len(),
                orphans.join(", ")
            ),
            format!(
                "Inspect {} and remove what you did not mean to keep.",
                cfg.store_dir.display()
            ),
        ));
    }

    if let Some(check) = leftover_cask(
        self_update::cask_dir().as_deref(),
        state.get(self_update::SELF_NAME).is_some(),
    ) {
        checks.push(check);
    }

    checks
}

/// Store directories whose names are not an installed package.
fn orphan_store_dirs(store: &Path, known: &BTreeSet<String>) -> Vec<String> {
    let mut names = Vec::new();
    let Ok(entries) = std::fs::read_dir(store) else {
        return names;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.starts_with('.') || !entry.path().is_dir() {
            continue;
        }
        if !known.contains(name) {
            names.push(name.to_string());
        }
    }
    names.sort();
    names
}

/// A Homebrew cask left behind after `ketch self uninstall` removed the rest.
fn leftover_cask(cask: Option<&Path>, ketch_installed: bool) -> Option<DoctorCheck> {
    let cask = cask?;
    if ketch_installed {
        return None;
    }
    Some(DoctorCheck::warn(
        "cask",
        format!("Homebrew still has {}", cask.display()),
        "Run `brew uninstall --cask ketch`.",
    ))
}

/// A `.lock` from a running ketch, or one a crashed run left behind.
fn lock_check(cfg: &Config) -> Option<DoctorCheck> {
    lock_check_at(&cfg.lock_file, crate::state::process_alive)
}

fn lock_check_at(path: &Path, alive: impl Fn(u32) -> bool) -> Option<DoctorCheck> {
    if !path.exists() {
        return None;
    }
    let holder = std::fs::read_to_string(path)
        .ok()
        .and_then(|t| t.trim().parse::<u32>().ok());
    match holder {
        Some(pid) if alive(pid) => Some(DoctorCheck::warn(
            "lock",
            format!("held by pid {pid}"),
            "Wait for the other ketch to finish.",
        )),
        Some(pid) => Some(DoctorCheck::warn(
            "lock",
            format!("stale (pid {pid} is gone)"),
            format!("Remove {}.", path.display()),
        )),
        None => Some(DoctorCheck::warn(
            "lock",
            format!("{} is unreadable", path.display()),
            format!("Remove {}.", path.display()),
        )),
    }
}

/// The `ketch` (or `ketch.exe`) the store is supposed to expose on PATH.
pub fn store_ketch_link(cfg: &Config) -> PathBuf {
    cfg.bin_dir
        .join(if cfg!(windows) { "ketch.exe" } else { "ketch" })
}

/// Whether two paths name the same file, after resolving links.
pub fn same_binary(a: &Path, b: &Path) -> bool {
    let left = dunce::canonicalize(a).unwrap_or_else(|_| a.to_path_buf());
    let right = dunce::canonicalize(b).unwrap_or_else(|_| b.to_path_buf());
    if cfg!(windows) {
        shell::windows_path_key(&left) == shell::windows_path_key(&right)
    } else {
        left == right
    }
}

/// First `ketch`/`ketch.exe` on PATH, canonicalised when the file exists.
pub fn first_ketch_on_path() -> Option<PathBuf> {
    first_ketch_on_path_from(std::env::var_os("PATH")?)
}

fn first_ketch_on_path_from(path: impl AsRef<std::ffi::OsStr>) -> Option<PathBuf> {
    let name = if cfg!(windows) { "ketch.exe" } else { "ketch" };
    for dir in std::env::split_paths(path.as_ref()) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(dunce::canonicalize(&candidate).unwrap_or(candidate));
        }
    }
    None
}

/// Warn when PATH finds a ketch that is not the store link.
///
/// `cargo install ketch` (or a leftover bootstrap) often lands in
/// `~/.cargo/bin` *ahead* of `~/.ketch/bin`. Then `ketch --version` prints
/// that old binary's crate version — historically `0.1.0` — even though
/// Cargo.toml and the store copy are 0.4.x. The PATH check only asks whether
/// the bin dir is present, not whether it wins.
fn path_binary_check(cfg: &Config) -> Option<DoctorCheck> {
    path_binary_check_from(cfg, std::env::var_os("PATH")?)
}

fn path_binary_check_from(cfg: &Config, path: impl AsRef<std::ffi::OsStr>) -> Option<DoctorCheck> {
    let linked = store_ketch_link(cfg);
    if !linked.exists() {
        return None;
    }
    let on_path = first_ketch_on_path_from(path.as_ref())?;
    if same_binary(&on_path, &linked) {
        return None;
    }
    Some(DoctorCheck::warn(
        "binary",
        format!(
            "PATH runs {} before {}",
            on_path.display(),
            linked.display()
        ),
        "Remove or rename the earlier copy (often ~/.cargo/bin/ketch from `cargo install`), or put ~/.ketch/bin first on PATH.",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::Report;

    fn scratch_config(root: &Path) -> Config {
        Config::load(Some(root.to_path_buf()), &Report::silent()).unwrap()
    }

    #[test]
    fn orphan_store_dirs_are_names_not_in_state() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("ghost")).unwrap();
        std::fs::create_dir(tmp.path().join("ripgrep")).unwrap();
        std::fs::write(tmp.path().join("file"), b"x").unwrap();
        let known = ["ripgrep".into()].into_iter().collect();
        assert_eq!(orphan_store_dirs(tmp.path(), &known), vec!["ghost"]);
    }

    #[test]
    fn registry_check_reports_age_from_meta_without_a_network() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = scratch_config(tmp.path());
        std::fs::create_dir_all(&cfg.registry_dir).unwrap();
        let pkg = cfg.registry_dir.join("jq");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(pkg.join("ketch.toml"), "source = \"github:jqlang/jq\"\n").unwrap();
        let fetched_at = crate::model::now_unix().saturating_sub(7200);
        std::fs::write(
            &cfg.registry_meta,
            format!("repo = \"{}\"\nfetched_at = {fetched_at}\n", cfg.registry),
        )
        .unwrap();
        let report = Report::silent();
        let check = registry_check(&Ctx::new(&cfg, &report));
        assert_eq!(check.status, CheckStatus::Ok);
        assert!(check.detail.contains("1 package"), "{}", check.detail);
        assert!(check.detail.contains("h ago"), "{}", check.detail);
    }

    #[test]
    fn a_scratch_root_has_every_check_and_no_failure_from_the_store() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = scratch_config(tmp.path());
        let report = Report::silent();
        let checks = checks(&Ctx::new(&cfg, &report));
        assert_eq!(checks[0].name, "version");
        let packages = checks.iter().find(|c| c.name == "packages").unwrap();
        assert_eq!(packages.status, CheckStatus::Ok);
        assert_eq!(packages.detail, "0 installed");
    }

    #[test]
    fn failed_counts_only_failures() {
        let checks = vec![
            DoctorCheck::ok("version", "ketch"),
            DoctorCheck::warn("links", "1 broken", "ketch link x"),
            DoctorCheck::fail("packages", "1 missing", "ketch install --force x"),
        ];
        assert_eq!(failed(&checks), 1);
    }

    #[test]
    fn path_binary_check_is_silent_when_the_store_link_wins() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = scratch_config(tmp.path());
        std::fs::create_dir_all(&cfg.bin_dir).unwrap();
        let linked = store_ketch_link(&cfg);
        std::fs::write(&linked, b"store").unwrap();
        let path = std::env::join_paths([&cfg.bin_dir]).unwrap();
        assert!(path_binary_check_from(&cfg, &path).is_none());
    }

    #[test]
    fn same_binary_folds_windows_drive_letter_case() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("ketch.bin");
        std::fs::write(&file, b"x").unwrap();
        let canon = dunce::canonicalize(&file).unwrap();
        let mut chars: Vec<char> = canon.to_string_lossy().chars().collect();
        if let Some(c) = chars.first_mut() {
            if c.is_ascii_uppercase() {
                *c = c.to_ascii_lowercase();
            } else if c.is_ascii_lowercase() {
                *c = c.to_ascii_uppercase();
            }
        }
        let flipped = PathBuf::from(chars.into_iter().collect::<String>());
        assert!(
            same_binary(&canon, &flipped),
            "case-only difference must still be the same binary: {} vs {}",
            canon.display(),
            flipped.display()
        );
    }

    #[test]
    fn path_binary_check_warns_when_an_earlier_ketch_shadows_the_store() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = scratch_config(tmp.path());
        std::fs::create_dir_all(&cfg.bin_dir).unwrap();
        let linked = store_ketch_link(&cfg);
        std::fs::write(&linked, b"store").unwrap();
        let shadow_dir = tmp.path().join("cargo-bin");
        std::fs::create_dir_all(&shadow_dir).unwrap();
        let shadow = shadow_dir.join(if cfg!(windows) { "ketch.exe" } else { "ketch" });
        std::fs::write(&shadow, b"stale").unwrap();
        let path = std::env::join_paths([&shadow_dir, &cfg.bin_dir]).unwrap();
        let check = path_binary_check_from(&cfg, &path).expect("shadowed");
        assert_eq!(check.status, CheckStatus::Warn);
        assert!(check.detail.contains("PATH runs"), "{}", check.detail);
        assert!(
            check.fix.as_deref().unwrap_or("").contains("cargo/bin"),
            "{:?}",
            check.fix
        );
    }

    #[test]
    fn leftover_cask_is_silent_when_ketch_is_still_installed() {
        assert!(leftover_cask(Some(Path::new("/opt/homebrew/Caskroom/ketch")), true).is_none());
        assert!(leftover_cask(None, false).is_none());
    }

    #[test]
    fn leftover_cask_warns_when_the_packages_are_gone() {
        let check = leftover_cask(Some(Path::new("/opt/homebrew/Caskroom/ketch")), false).unwrap();
        assert_eq!(check.name, "cask");
        assert_eq!(check.status, CheckStatus::Warn);
        assert!(check.detail.contains("Caskroom"));
    }

    #[test]
    fn a_missing_lock_file_is_not_a_check() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(lock_check_at(&tmp.path().join(".lock"), |_| true).is_none());
    }

    #[test]
    fn a_stale_lock_file_is_a_warning() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(".lock");
        std::fs::write(&path, "12345\n").unwrap();
        let check = lock_check_at(&path, |_| false).unwrap();
        assert_eq!(check.status, CheckStatus::Warn);
        assert!(check.detail.contains("stale"));
        assert!(check.detail.contains("12345"));
    }

    #[test]
    fn a_held_lock_file_names_the_pid() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(".lock");
        std::fs::write(&path, "9").unwrap();
        let check = lock_check_at(&path, |pid| pid == 9).unwrap();
        assert!(check.detail.contains("pid 9"));
        assert!(!check.detail.contains("stale"));
    }
}
