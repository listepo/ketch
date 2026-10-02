// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

//! `ketch import`: a package another manager knows, added the ketch way.
//!
//! The source's definition is converted to a user manifest, written to the
//! manifest directory, and installed through the same path `ketch install`
//! takes. Everything about what changes on a re-run is decided in the core
//! (`import::apply`); this file only fetches, prints and commits.

use crate::cancel::Cancel;
use crate::cli::{ImportCommand, ImportOpts};
use crate::config::Config;
use crate::error::{Error, Result};
use crate::import::apply::{self, Apply, Have, Step};
use crate::import::{brew, linux, winget, Endpoints, Found, HttpFetch};
use crate::install::{self, InstallRequest};
use crate::model::PackageSpec;
use crate::source::SourceRegistry;
use crate::state::{Lock, State};
use crate::ui;

pub fn run(cfg: &Config, command: ImportCommand) -> Result<()> {
    let endpoints = Endpoints::from_env();
    let fetch = HttpFetch::new(&ui::ctx(cfg), &endpoints);
    let (typed, opts, found) = match &command {
        ImportCommand::Winget { id, opts } => (id, opts, winget::lookup(&fetch, &endpoints, id)),
        ImportCommand::Brew {
            name,
            cask,
            formula,
            opts,
        } => {
            let pick = match (cask, formula) {
                (true, _) => brew::Pick::Cask,
                (_, true) => brew::Pick::Formula,
                _ => brew::Pick::Either,
            };
            (name, opts, brew::lookup(&fetch, &endpoints, name, pick))
        }
        ImportCommand::Linux { name, opts } => {
            (name, opts, linux::lookup(&fetch, &endpoints, name))
        }
    };
    let found = found.map_err(|e| refusal(typed, e))?;
    apply_found(cfg, opts, found)
}

/// A package that does not convert is the expected answer for most of
/// every catalogue, not a fault in ketch: its sentence is printed as it is,
/// with no `error:` label or hint in front of it.
fn refusal(typed: &str, err: Error) -> Error {
    let text = err.to_string();
    if text.starts_with(&format!("{typed} can't be converted: ")) {
        ui::bare_error(&text);
        return Error::Reported(1);
    }
    err
}

fn apply_found(cfg: &Config, opts: &ImportOpts, found: Found) -> Result<()> {
    let Found { converted, notes } = found;
    for line in &notes {
        ui::note(line);
    }
    let rendered = converted.render()?;
    let path = crate::manifest::user_manifest_path(cfg, &converted.name);
    let label = path.display().to_string();
    let source = format!("{} {}", converted.backend, converted.package);

    // Taken before the file is read, so two imports cannot both decide
    // against the same old copy.
    let _lock = if opts.dry_run {
        None
    } else {
        Some(Lock::acquire(&ui::ctx(cfg))?)
    };
    let on_disk = match std::fs::read_to_string(&path) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(Error::io(&path, e)),
    };
    let mut state = State::load(cfg)?;
    let installed = state.get(&converted.name).map(|p| Have {
        version: &p.version,
        tag: &p.tag,
    });
    let step = apply::decide(
        &converted,
        &rendered,
        on_disk.as_deref(),
        installed,
        &cfg.target,
        &label,
    )?;

    let plan = match step {
        Step::UpToDate => {
            ui::success(
                "up to date",
                &format!(
                    "Everything is up to date: {} {} from {source}",
                    converted.name, converted.version
                ),
            );
            return Ok(());
        }
        Step::Apply(plan) => plan,
    };

    if opts.dry_run {
        ui::out(rendered.trim_end());
        match &plan.write {
            Some(_) => ui::note(&format!("would write {label}")),
            None => ui::note(&format!("{label} already holds this")),
        }
        ui::note(&format!(
            "would install {} {}",
            converted.name, plan.version
        ));
        return Ok(());
    }

    if plan.unchecked {
        ui::warn(&format!(
            "the {source} definition records no checksum for this download; \
             ketch checks it the way it checks any release"
        ));
    }
    if let Some(text) = &plan.write {
        crate::manifest::write_manifest(&path, text)?;
        ui::success("wrote", &label);
    }

    let req = request(cfg, &converted.name, &plan);
    let sources = SourceRegistry::load(&ui::ctx(cfg));
    let out = install::install(&ui::ctx_asking(cfg, !opts.yes), &sources, &mut state, &req)?;
    state.save(cfg)?;
    super::pkg::report(&out);
    super::pkg::path_hint(cfg, &state);
    Ok(())
}

fn request(cfg: &Config, name: &str, plan: &Apply) -> InstallRequest {
    let mut spec = PackageSpec::parse(name);
    spec.version = plan.version.clone();
    InstallRequest {
        spec,
        force: plan.force,
        prerelease: false,
        link: true,
        require_checksum: cfg.require_checksums,
        asset_override: plan.asset.clone(),
        expected_sha256: plan.sha256.clone(),
        name_override: None,
        bin: None,
        locked_bin: None,
        // The version is the one the source names; asking whether to take
        // a newer one would be a different command.
        offer_update: false,
        cancel: Cancel::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_for_the_typed_name_is_printed_bare() {
        let err = Error::msg(crate::import::not_github_message("jq"));
        assert!(matches!(refusal("jq", err), Error::Reported(1)));
    }

    #[test]
    fn any_other_failure_keeps_its_error() {
        let err = Error::msg("Arch Linux has no package named `nope`");
        assert!(matches!(refusal("nope", err), Error::Msg(_)));
    }
}
