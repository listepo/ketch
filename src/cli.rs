//! Command-line surface.
//!
//! Kept separate from `main.rs` so command implementations in `cmd/` can take
//! their own argument struct directly, with no re-packing in between.

use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "ketch",
    version,
    about = "Catch releases straight from GitHub.",
    long_about = "ketch installs command-line tools and macOS apps directly from GitHub releases.\n\
                  No taps, no formulae, no build step — it downloads what the project already ships.",
    propagate_version = true,
    disable_help_subcommand = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    #[command(flatten)]
    pub global: GlobalArgs,
}

#[derive(Args, Debug, Clone)]
pub struct GlobalArgs {
    /// ketch root directory (default: ~/.ketch)
    #[arg(long, global = true, value_name = "DIR")]
    pub root: Option<PathBuf>,

    /// Show what ketch is doing, including every request
    #[arg(long, short, global = true, conflicts_with = "quiet")]
    pub verbose: bool,

    /// Only print errors and requested data
    #[arg(long, short, global = true)]
    pub quiet: bool,

    /// Never emit ANSI colour
    #[arg(long, global = true)]
    pub no_color: bool,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Install one or more packages
    #[command(visible_alias = "i")]
    Install(InstallArgs),

    /// Remove installed packages
    #[command(visible_aliases = ["remove", "rm"])]
    Uninstall(UninstallArgs),

    /// Show installed packages
    #[command(visible_alias = "ls")]
    List(ListArgs),

    /// Show installed packages that have a newer release
    Outdated(OutdatedArgs),

    /// Show details about a package, installed or not
    #[command(visible_alias = "show")]
    Info(InfoArgs),

    /// Show what changed: the package's own changelog, or its release notes
    Changelog(ChangelogArgs),

    /// Search GitHub for installable repositories
    Search(SearchArgs),

    /// Refresh the package registry (see `upgrade` for installed packages)
    Update,

    /// Upgrade installed packages to their latest release
    Upgrade(UpgradeArgs),

    /// Hold a package at its current version
    Pin(NameArgs),

    /// Release a pin
    Unpin(NameArgs),

    /// Re-create the links for an installed package
    Link(NameArgs),

    /// Remove the links for an installed package, keeping it installed
    Unlink(NameArgs),

    /// Write or check `ketch.lock`, a reproducible record of what is installed
    Lock(LockArgs),

    /// Install everything `ketch.lock` names, at the versions it names
    Sync(SyncArgs),

    /// Check the environment and the install tree
    Doctor(DoctorArgs),

    /// Put the ketch bin directory on your shell's PATH
    Path {
        #[command(subcommand)]
        command: Option<PathCommand>,
    },

    /// Manage source plugins
    Plugin {
        #[command(subcommand)]
        command: PluginCommand,
    },

    /// Manage ketch itself
    #[command(name = "self")]
    Zelf {
        #[command(subcommand)]
        command: SelfCommand,
    },

    /// Print a shell completion script
    Completions(CompletionsArgs),
}

#[derive(Args, Debug, Clone)]
pub struct InstallArgs {
    /// `owner/repo`, `scheme:id`, or a known alias — each may carry `@version`
    #[arg(required = true, value_name = "PKG")]
    pub packages: Vec<String>,

    /// Reinstall even when the requested version is already present
    #[arg(long, short)]
    pub force: bool,

    /// Consider prereleases when resolving `latest`
    #[arg(long = "pre")]
    pub prerelease: bool,

    /// Unpack and record the package without putting it on PATH
    #[arg(long)]
    pub no_link: bool,

    /// Refuse to install unless the release publishes a checksum
    #[arg(long)]
    pub require_checksum: bool,

    /// Use this release asset by exact file name instead of auto-selecting
    #[arg(long, value_name = "NAME")]
    pub asset: Option<String>,

    /// Packages to work on at once (default: 4, or `jobs` in config.toml)
    #[arg(long, short = 'j', value_name = "N")]
    pub jobs: Option<usize>,

    /// Answer yes to every prompt
    #[arg(long, short = 'y')]
    pub yes: bool,
}

#[derive(Args, Debug, Clone)]
pub struct UninstallArgs {
    #[arg(required = true, value_name = "NAME")]
    pub names: Vec<String>,

    /// Answer yes to every prompt
    #[arg(long, short = 'y')]
    pub yes: bool,
}

#[derive(Args, Debug, Clone)]
pub struct ListArgs {
    /// Emit JSON instead of a table
    #[arg(long)]
    pub json: bool,

    /// Print only package names, one per line
    #[arg(long, conflicts_with = "json")]
    pub names_only: bool,
}

#[derive(Args, Debug, Clone)]
pub struct OutdatedArgs {
    /// Emit JSON instead of a table
    #[arg(long)]
    pub json: bool,

    /// Compare against prereleases too
    #[arg(long = "pre")]
    pub prerelease: bool,
}

#[derive(Args, Debug, Clone)]
pub struct InfoArgs {
    /// An installed name, an alias, or `owner/repo`
    #[arg(value_name = "PKG")]
    pub package: String,

    /// Emit JSON instead of formatted text
    #[arg(long)]
    pub json: bool,

    /// List the release's assets and how each one scored
    #[arg(long)]
    pub assets: bool,
}

#[derive(Args, Debug, Clone)]
pub struct ChangelogArgs {
    /// An installed name, an alias, or `owner/repo` — may carry `@version`
    #[arg(value_name = "PKG")]
    pub package: String,

    /// Show the newest release instead of the installed one
    #[arg(long)]
    pub latest: bool,

    /// Only read the changelog file the package ships
    #[arg(long, conflicts_with_all = ["release", "latest"])]
    pub file: bool,

    /// Only read the notes published with the release
    #[arg(long)]
    pub release: bool,
}

#[derive(Args, Debug, Clone)]
pub struct SearchArgs {
    #[arg(required = true, value_name = "QUERY")]
    pub query: Vec<String>,

    /// Maximum results
    #[arg(long, short = 'n', default_value_t = 15)]
    pub limit: usize,
}

#[derive(Args, Debug, Clone)]
pub struct UpgradeArgs {
    /// Packages to upgrade. Empty means every unpinned package.
    #[arg(value_name = "NAME")]
    pub names: Vec<String>,

    /// Report what would change without touching anything
    #[arg(long)]
    pub dry_run: bool,

    /// Consider prereleases
    #[arg(long = "pre")]
    pub prerelease: bool,

    /// Upgrade pinned packages too
    #[arg(long)]
    pub force: bool,

    /// Packages to work on at once (default: 4, or `jobs` in config.toml)
    #[arg(long, short = 'j', value_name = "N")]
    pub jobs: Option<usize>,

    /// Answer yes to every prompt
    #[arg(long, short = 'y')]
    pub yes: bool,
}

#[derive(Args, Debug, Clone)]
pub struct NameArgs {
    #[arg(required = true, value_name = "NAME")]
    pub names: Vec<String>,
}

#[derive(Args, Debug, Clone)]
pub struct LockArgs {
    /// Lockfile to write (default: ./ketch.lock)
    #[arg(long, short, value_name = "FILE")]
    pub file: Option<PathBuf>,

    /// Report how the tree differs from the lockfile, and write nothing
    #[arg(long)]
    pub check: bool,
}

#[derive(Args, Debug, Clone)]
pub struct SyncArgs {
    /// Lockfile to read (default: ./ketch.lock)
    #[arg(long, short, value_name = "FILE")]
    pub file: Option<PathBuf>,

    /// Also remove installed packages the lockfile does not name
    #[arg(long)]
    pub prune: bool,

    /// Report what would change without installing anything
    #[arg(long)]
    pub dry_run: bool,

    /// Packages to work on at once (default: 4, or `jobs` in config.toml)
    #[arg(long, short = 'j', value_name = "N")]
    pub jobs: Option<usize>,

    /// Answer yes to every prompt
    #[arg(long, short = 'y')]
    pub yes: bool,
}

#[derive(Args, Debug, Clone)]
pub struct DoctorArgs {
    /// Repair what can be repaired without asking: currently the PATH setup
    #[arg(long)]
    pub fix: bool,
}

#[derive(Subcommand, Debug, Clone)]
pub enum PathCommand {
    /// Add the bin directory to your shell's startup file
    Install(PathInstallArgs),
    /// Take out the block `ketch path install` added
    Uninstall(PathArgs),
    /// Show which shells have been set up
    Status,
}

#[derive(Args, Debug, Clone)]
pub struct PathArgs {
    /// Shells to act on. Default: the ones you appear to use.
    #[arg(long, value_name = "SHELL", value_enum)]
    pub shell: Vec<crate::shell::Shell>,

    /// Act on every shell ketch knows
    #[arg(long, conflicts_with = "shell")]
    pub all: bool,

    /// Report what would change without touching anything
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args, Debug, Clone)]
pub struct PathInstallArgs {
    #[command(flatten)]
    pub common: PathArgs,

    /// Print the line to add by hand instead of editing anything
    #[arg(long, conflicts_with_all = ["all", "dry_run"])]
    pub print: bool,
}

#[derive(Subcommand, Debug, Clone)]
pub enum PluginCommand {
    /// Show discovered source plugins
    List {
        /// Emit JSON instead of a table
        #[arg(long)]
        json: bool,
    },
    /// Print the directory plugins are loaded from
    Dir,
}

#[derive(Subcommand, Debug, Clone)]
pub enum SelfCommand {
    /// Replace this binary with the latest release
    Update {
        /// Report what would happen without replacing anything
        #[arg(long)]
        dry_run: bool,
        /// Reinstall even when already current
        #[arg(long, short)]
        force: bool,
    },
    /// Print the running version and where it lives
    Version,
    /// Remove ketch itself
    Uninstall {
        /// Also delete the store, cache and state
        #[arg(long)]
        purge: bool,
        /// Answer yes to every prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
}

#[derive(Args, Debug, Clone)]
pub struct CompletionsArgs {
    #[arg(value_enum)]
    pub shell: clap_complete::Shell,
}
