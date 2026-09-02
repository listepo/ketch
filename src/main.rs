//! ketch — catch releases straight from GitHub.
//!
//! `main` does three things and nothing else: parse arguments, build the
//! `Config`, and hand off to a command. Every failure path converges here so a
//! single place decides how errors are shown and what the process exits with.

mod changelog;
mod cli;
mod cmd;
mod config;
mod error;
mod extract;
mod http;
mod install;
mod lockfile;
mod log;
mod manifest;
mod model;
mod platform;
mod registry;
mod selfupdate;
mod shell;
mod source;
mod state;
#[cfg(feature = "tui")]
mod tui;
mod ui;

use clap::{CommandFactory, Parser};
use cli::{Cli, Command};
use error::Result;

fn main() {
    let cli = Cli::parse();
    ui::init(
        if cli.global.no_color {
            Some(false)
        } else {
            None
        },
        cli.global.quiet,
        cli.global.verbose,
    );

    if let Err(err) = run(cli) {
        ui::error(&err);
        // What npm and cargo do, and for the same reason: the terminal shows
        // the failure, the log shows the run that led to it.
        if let Some(path) = log::path() {
            ui::note(&format!(
                "the full log of this run is in {}",
                path.display()
            ));
        }
        std::process::exit(err.exit_code());
    }
}

fn run(cli: Cli) -> Result<()> {
    // Completions must work before any directory exists, so it is handled
    // before the config is built.
    if let Command::Completions(args) = &cli.command {
        let mut command = Cli::command();
        let name = command.get_name().to_string();
        clap_complete::generate(args.shell, &mut command, name, &mut std::io::stdout());
        return Ok(());
    }

    let cfg = config::Config::load(cli.global.root.clone())?;
    cfg.ensure_dirs()?;
    log::init(&cfg);

    #[cfg(feature = "tui")]
    let _tui = start_tui(&cli);
    ui::debug(&format!(
        "root {} · target {} · token {}",
        cfg.root.display(),
        cfg.target,
        if cfg.github_token.is_some() {
            "yes"
        } else {
            "no"
        }
    ));

    match cli.command {
        Command::Install(args) => cmd::pkg::install(&cfg, args),
        Command::Uninstall(args) => cmd::pkg::uninstall(&cfg, args),
        Command::Upgrade(args) => cmd::pkg::upgrade(&cfg, args),
        Command::Pin(args) => cmd::pkg::pin(&cfg, args, true),
        Command::Unpin(args) => cmd::pkg::pin(&cfg, args, false),
        Command::Link(args) => cmd::pkg::link(&cfg, args, true),
        Command::Unlink(args) => cmd::pkg::link(&cfg, args, false),
        Command::List(args) => cmd::query::list(&cfg, args),
        Command::Outdated(args) => cmd::query::outdated(&cfg, args),
        Command::Info(args) => cmd::query::info(&cfg, args),
        Command::Changelog(args) => cmd::query::changelog(&cfg, args),
        Command::Search(args) => cmd::query::search(&cfg, args),
        Command::Update => cmd::system::update(&cfg),
        Command::Lock(args) => cmd::lock::lock(&cfg, args),
        Command::Sync(args) => cmd::lock::sync(&cfg, args),
        Command::Doctor(args) => cmd::system::doctor(&cfg, args),
        Command::Path { command } => cmd::system::path(&cfg, command),
        Command::Plugin { command } => cmd::system::plugin(&cfg, command),
        Command::Zelf { command } => cmd::system::zelf(&cfg, command),
        Command::Completions(_) => unreachable!("handled above"),
    }
}

/// Start an opt-in session only for commands that have long-running progress.
#[cfg(feature = "tui")]
fn start_tui(cli: &Cli) -> Option<tui::Session> {
    if !cli.global.tui || !tui::can_start(cli.global.quiet) {
        return None;
    }
    let (command, packages) = match &cli.command {
        Command::Install(args) => ("install", args.packages.clone()),
        Command::Upgrade(args) => ("upgrade", args.names.clone()),
        Command::Sync(_) => ("sync", Vec::new()),
        Command::Update => ("update", vec!["registry".to_string()]),
        // `--tui` is global so scripts do not need a command-specific spelling,
        // but a read-only command has no progress stream to render.
        _ => return None,
    };
    match tui::Session::start(command, packages) {
        Ok(session) => {
            ui::enable_tui(session.controller());
            Some(session)
        }
        // An explicitly requested TUI must still leave a useful CLI on a
        // terminal that refuses raw mode or an alternate screen.
        Err(error) => {
            ui::debug(&format!("TUI unavailable; using line output: {error}"));
            None
        }
    }
}
