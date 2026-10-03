// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

//! The contract scenarios under `desktop/contract/scenarios/`, generated here.
//!
//! The macOS, Windows and Linux apps each test against a fake core. A fake that
//! invents its own records and event streams drifts from the real core and from
//! the other two, so all three read the same JSON files, and those files are
//! built from the Rust values a foreign caller receives: a field renamed or
//! dropped in `ketch-ffi` breaks this file's compile, and a changed wire shape
//! fails the comparison. `KETCH_BLESS=1 cargo nextest run -p ketch-ffi contract`
//! rewrites the files, the way the schema tests do.
//!
//! A scenario is one call, what the core said and asked while it ran, and how
//! it ended. The events are hand-ordered as the pipeline emits them, not
//! captured from a run: a captured one would carry timestamps and temp paths
//! no fake could use.

use ketch_ffi::records::{
    Changelog, ChangelogSource, Check, CheckOutcome, InstallOptions, Installed, Package,
    RegistryPackage, Repository, SearchResults, Upgrade,
};
use ketch_ffi::{Event, Holder, KetchError, Stage, TaskKind};
use pretty_assertions::assert_eq;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// The call a scenario answers, with the arguments the app passes.
#[derive(Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum Call {
    Installed,
    Search {
        query: String,
        limit: u32,
    },
    Outdated,
    Install {
        specs: Vec<String>,
        options: InstallOptions,
    },
    Upgrade {
        names: Vec<String>,
    },
    Uninstall {
        names: Vec<String>,
    },
    ChangelogRange {
        package: String,
        from: Option<String>,
        to: Option<String>,
    },
    Doctor,
}

/// A question the core puts to the app mid-operation.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Question {
    ChooseBinary {
        package: String,
        candidates: Vec<String>,
    },
    StopProcesses {
        holders: Vec<Holder>,
    },
}

/// What the app answers: the picked index (`None` declines), or yes or no.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Answer {
    Index { index: Option<u32> },
    Confirm { confirmed: bool },
}

/// One thing that happens during the call, in order.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Step {
    Event { event: Event },
    Ask { question: Question, answer: Answer },
}

/// How the call ended: the value it returned, or the error.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Outcome {
    Ok { value: serde_json::Value },
    Error { error: KetchError },
}

#[derive(Serialize)]
struct Scenario {
    name: &'static str,
    description: &'static str,
    call: Call,
    script: Vec<Step>,
    outcome: Outcome,
}

fn ok(value: impl Serialize) -> Outcome {
    Outcome::Ok {
        value: serde_json::to_value(value).expect("a record serialises"),
    }
}

fn event(event: Event) -> Step {
    Step::Event { event }
}

fn package(name: &str, version: &str, repo: &str) -> Package {
    Package {
        name: name.into(),
        version: version.into(),
        tag: version.into(),
        source: format!("github:{repo}"),
        pinned: false,
        retained: Vec::new(),
        installed_at: 1_760_000_000,
        prefix: format!("/Users/me/.ketch/store/{name}/{version}"),
        binaries: vec![format!("/Users/me/.ketch/bin/{name}")],
        trust: "checksum".into(),
    }
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

/// The stream of a clean single-package install: resolve, download with
/// progress, verify, extract, install, done.
fn install_stream(name: &str, version: &str) -> Vec<Step> {
    vec![
        event(Event::Step {
            package: name.into(),
            stage: Stage::Resolving,
        }),
        event(Event::Began {
            id: 1,
            task: TaskKind::Download {
                label: name.into(),
                batch_id: None,
            },
        }),
        event(Event::Sized {
            id: 1,
            name: format!("{name}-{version}.tar.gz"),
            total: Some(2_000_000),
        }),
        event(Event::Step {
            package: name.into(),
            stage: Stage::Downloading,
        }),
        event(Event::Progress {
            id: 1,
            done: 500_000,
            total: Some(2_000_000),
        }),
        event(Event::Progress {
            id: 1,
            done: 2_000_000,
            total: Some(2_000_000),
        }),
        event(Event::Ended {
            id: 1,
            message: None,
        }),
        event(Event::Step {
            package: name.into(),
            stage: Stage::Verifying,
        }),
        event(Event::Step {
            package: name.into(),
            stage: Stage::Extracting,
        }),
        event(Event::Step {
            package: name.into(),
            stage: Stage::Installing,
        }),
        event(Event::Success {
            verb: "installed".into(),
            detail: format!("{name} {version}"),
        }),
    ]
}

/// A download that started, got part of the way and was dropped.
fn abandoned_download(name: &str) -> Vec<Step> {
    vec![
        event(Event::Step {
            package: name.into(),
            stage: Stage::Resolving,
        }),
        event(Event::Began {
            id: 1,
            task: TaskKind::Download {
                label: name.into(),
                batch_id: None,
            },
        }),
        event(Event::Sized {
            id: 1,
            name: format!("{name}.tar.gz"),
            total: Some(2_000_000),
        }),
        event(Event::Step {
            package: name.into(),
            stage: Stage::Downloading,
        }),
        event(Event::Progress {
            id: 1,
            done: 700_000,
            total: Some(2_000_000),
        }),
        event(Event::Abandoned { id: 1 }),
    ]
}

fn install_call(spec: &str) -> Call {
    Call::Install {
        specs: strings(&[spec]),
        options: InstallOptions::default(),
    }
}

fn scenarios() -> Vec<Scenario> {
    let mut pinned = package("fd", "10.1.0", "sharkdp/fd");
    pinned.pinned = true;
    pinned.retained = strings(&["10.0.0"]);
    let ripgrep = package("ripgrep", "14.1.0", "BurntSushi/ripgrep");
    let ripgrep_new = package("ripgrep", "14.1.1", "BurntSushi/ripgrep");

    vec![
        Scenario {
            name: "installed",
            description: "Two installed packages, one of them pinned with an older version kept.",
            call: Call::Installed,
            script: vec![],
            outcome: ok(vec![pinned.clone(), ripgrep.clone()]),
        },
        Scenario {
            name: "search",
            description: "A search with a curated package whose latest version is unknown, and a repository.",
            call: Call::Search {
                query: "grep".into(),
                limit: 10,
            },
            script: vec![],
            outcome: ok(SearchResults {
                known: vec![
                    RegistryPackage {
                        name: "ripgrep".into(),
                        source: "github:BurntSushi/ripgrep".into(),
                        description: Some("Line-oriented search tool".into()),
                        latest: Some("14.1.1".into()),
                    },
                    RegistryPackage {
                        name: "ugrep".into(),
                        source: "github:Genivia/ugrep".into(),
                        description: None,
                        latest: None,
                    },
                ],
                repositories: vec![Repository {
                    spec: "github:phiresky/ripgrep-all".into(),
                    stars: Some(12_000),
                    description: Some("rga: ripgrep, but also search in PDFs".into()),
                }],
            }),
        },
        Scenario {
            name: "outdated",
            description: "One upgrade available and one held back by a pin.",
            call: Call::Outdated,
            script: vec![],
            outcome: ok(vec![
                Upgrade {
                    name: "ripgrep".into(),
                    installed: "14.1.0".into(),
                    latest: "14.1.1".into(),
                    tag: "14.1.1".into(),
                    pinned: false,
                    held_by: None,
                },
                Upgrade {
                    name: "fd".into(),
                    installed: "10.1.0".into(),
                    latest: "10.2.0".into(),
                    tag: "v10.2.0".into(),
                    pinned: true,
                    held_by: Some("/work/app/ketch.lock".into()),
                },
            ]),
        },
        Scenario {
            name: "changelog-range",
            description: "Everything between the installed and the latest version, newest first: a section of a file, then release notes.",
            call: Call::ChangelogRange {
                package: "ripgrep".into(),
                from: Some("14.1.0".into()),
                to: None,
            },
            script: vec![],
            outcome: ok(vec![
                Changelog {
                    name: "ripgrep".into(),
                    version: "14.1.1".into(),
                    source: ChangelogSource::File {
                        path: "/Users/me/.ketch/store/ripgrep/14.1.1/CHANGELOG.md".into(),
                    },
                    heading: Some("14.1.1".into()),
                    body: "Bug fixes:\n\n- Fix a regression in `--json`.".into(),
                },
                Changelog {
                    name: "ripgrep".into(),
                    version: "14.1.0".into(),
                    source: ChangelogSource::Release,
                    heading: None,
                    body: "Performance improvements.".into(),
                },
            ]),
        },
        Scenario {
            name: "doctor",
            description: "One check of each outcome; a warning carries the fix to offer.",
            call: Call::Doctor,
            script: vec![],
            outcome: ok(vec![
                Check {
                    name: "path".into(),
                    outcome: CheckOutcome::Ok,
                    detail: "/Users/me/.ketch/bin is on PATH".into(),
                    fix: None,
                },
                Check {
                    name: "links".into(),
                    outcome: CheckOutcome::Warn,
                    detail: "1 broken link: /Users/me/.ketch/bin/old".into(),
                    fix: Some("ketch doctor --fix".into()),
                },
                Check {
                    name: "state".into(),
                    outcome: CheckOutcome::Fail,
                    detail: "state.json is not readable".into(),
                    fix: None,
                },
            ]),
        },
        Scenario {
            name: "install-ok",
            description: "A clean install: every stage in order, a download with progress, then the installed package.",
            call: install_call("ripgrep"),
            script: install_stream("ripgrep", "14.1.1"),
            outcome: ok(vec![Installed {
                package: ripgrep_new.clone(),
                replaced: None,
            }]),
        },
        Scenario {
            name: "install-cancelled",
            description: "The person cancels during the download: the task is abandoned, then Cancelled. Nothing is left installed, and Cancelled is logged rather than shown as an alert.",
            call: install_call("ripgrep"),
            script: abandoned_download("ripgrep"),
            outcome: Outcome::Error {
                error: KetchError::Cancelled,
            },
        },
        Scenario {
            name: "install-network-failure",
            description: "The download fails part way: the task is abandoned, then Network with the core's wording.",
            call: install_call("ripgrep"),
            script: abandoned_download("ripgrep"),
            outcome: Outcome::Error {
                error: KetchError::Network {
                    message: "HTTP 503 from https://github.com/BurntSushi/ripgrep/releases/download/14.1.1/ripgrep.tar.gz".into(),
                },
            },
        },
        Scenario {
            name: "install-verification-failure",
            description: "The download does not match its published checksum.",
            call: install_call("ripgrep"),
            script: vec![
                event(Event::Step {
                    package: "ripgrep".into(),
                    stage: Stage::Resolving,
                }),
                event(Event::Step {
                    package: "ripgrep".into(),
                    stage: Stage::Verifying,
                }),
            ],
            outcome: Outcome::Error {
                error: KetchError::Verification {
                    message: "checksum mismatch for ripgrep.tar.gz\nexpected aa11\nactual bb22".into(),
                },
            },
        },
        Scenario {
            name: "install-busy",
            description: "Another ketch process holds the lock: nothing runs, and the app offers Retry.",
            call: install_call("ripgrep"),
            script: vec![],
            outcome: Outcome::Error {
                error: KetchError::Busy { pid: Some(4242) },
            },
        },
        Scenario {
            name: "install-busy-unknown-holder",
            description: "The lock is held but its file names no process, so there is no pid to show.",
            call: install_call("ripgrep"),
            script: vec![],
            outcome: Outcome::Error {
                error: KetchError::Busy { pid: None },
            },
        },
        Scenario {
            name: "install-not-found",
            description: "No package or release answers to the name.",
            call: install_call("nope"),
            script: vec![],
            outcome: Outcome::Error {
                error: KetchError::NotFound {
                    name: "nope".into(),
                },
            },
        },
        Scenario {
            name: "install-other-error",
            description: "Any other failure is shown as an alert with the core's own wording.",
            call: install_call("local:/tmp/missing"),
            script: vec![],
            outcome: Outcome::Error {
                error: KetchError::Other {
                    message: "`/tmp/missing` does not exist".into(),
                },
            },
        },
        Scenario {
            name: "install-binary-choice",
            description: "Two binaries share the package's name, so the core asks which to link and the app answers with an index.",
            call: install_call("uv"),
            script: {
                let mut script = vec![
                    event(Event::Step {
                        package: "uv".into(),
                        stage: Stage::Resolving,
                    }),
                    event(Event::Step {
                        package: "uv".into(),
                        stage: Stage::Installing,
                    }),
                ];
                script.push(Step::Ask {
                    question: Question::ChooseBinary {
                        package: "uv".into(),
                        candidates: strings(&["uv", "uvx"]),
                    },
                    answer: Answer::Index { index: Some(1) },
                });
                script.push(event(Event::Success {
                    verb: "installed".into(),
                    detail: "uv 0.9.0".into(),
                }));
                script
            },
            outcome: ok(vec![Installed {
                package: package("uv", "0.9.0", "astral-sh/uv"),
                replaced: None,
            }]),
        },
        Scenario {
            name: "upgrade-stops-processes",
            description: "A running process holds a file the upgrade replaces: the core asks whether to stop it and the app says yes.",
            call: Call::Upgrade {
                names: strings(&["ripgrep"]),
            },
            script: {
                let mut script = vec![event(Event::Step {
                    package: "ripgrep".into(),
                    stage: Stage::Resolving,
                })];
                script.push(Step::Ask {
                    question: Question::StopProcesses {
                        holders: vec![Holder {
                            pid: 5150,
                            path: "/Users/me/.ketch/bin/rg".into(),
                        }],
                    },
                    answer: Answer::Confirm { confirmed: true },
                });
                script.push(event(Event::Success {
                    verb: "upgraded".into(),
                    detail: "ripgrep 14.1.0 -> 14.1.1".into(),
                }));
                script
            },
            outcome: ok(vec![Installed {
                package: ripgrep_new,
                replaced: Some("14.1.0".into()),
            }]),
        },
        Scenario {
            name: "upgrade-nothing-to-do",
            description: "Nothing is outdated, or everything outdated is pinned: no events, an empty result.",
            call: Call::Upgrade { names: Vec::new() },
            script: vec![],
            outcome: ok(Vec::<Installed>::new()),
        },
        Scenario {
            name: "uninstall-ok",
            description: "Removing an installed package returns the record that was removed.",
            call: Call::Uninstall {
                names: strings(&["ripgrep"]),
            },
            script: vec![event(Event::Success {
                verb: "uninstalled".into(),
                detail: "ripgrep 14.1.0".into(),
            })],
            outcome: ok(vec![ripgrep]),
        },
        Scenario {
            name: "uninstall-not-found",
            description: "Uninstalling a name that is not installed.",
            call: Call::Uninstall {
                names: strings(&["nope"]),
            },
            script: vec![],
            outcome: Outcome::Error {
                error: KetchError::NotFound {
                    name: "nope".into(),
                },
            },
        },
    ]
}

fn directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../desktop/contract/scenarios")
}

fn render(scenario: &Scenario) -> String {
    serde_json::to_string_pretty(scenario).expect("a scenario serialises") + "\n"
}

#[test]
fn the_committed_contract_scenarios_match_the_rust_types() {
    let dir = directory();
    let bless = std::env::var_os("KETCH_BLESS").is_some();
    let all = scenarios();
    if bless {
        std::fs::create_dir_all(&dir).expect("create the scenario directory");
    }
    for scenario in &all {
        let path = dir.join(format!("{}.json", scenario.name));
        let rendered = render(scenario);
        if bless {
            std::fs::write(&path, &rendered).expect("write a scenario");
            continue;
        }
        // A Windows checkout may have turned LF into CRLF; the scenario is the same.
        let committed = std::fs::read_to_string(&path)
            .unwrap_or_default()
            .replace("\r\n", "\n");
        assert_eq!(
            committed,
            rendered,
            "{} is stale; regenerate it with KETCH_BLESS=1 cargo nextest run -p ketch-ffi contract",
            path.display()
        );
    }
    // The other direction: a scenario removed here must not linger on disk and
    // go on feeding a fake with a call the core no longer makes.
    let expected: Vec<String> = all.iter().map(|s| format!("{}.json", s.name)).collect();
    let mut on_disk: Vec<String> = std::fs::read_dir(&dir)
        .expect("the scenario directory exists")
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| name.ends_with(".json"))
        .collect();
    on_disk.sort();
    let mut expected_sorted = expected;
    expected_sorted.sort();
    if bless {
        for stale in on_disk.iter().filter(|n| !expected_sorted.contains(n)) {
            std::fs::remove_file(dir.join(stale)).expect("remove a stale scenario");
        }
        return;
    }
    assert_eq!(on_disk, expected_sorted, "a scenario file has no generator");
}

#[test]
fn scenario_names_are_unique_and_file_safe() {
    let all = scenarios();
    let mut names: Vec<&str> = all.iter().map(|s| s.name).collect();
    names.sort_unstable();
    let count = names.len();
    names.dedup();
    assert_eq!(names.len(), count, "two scenarios share a name");
    assert!(
        names
            .iter()
            .all(|n| n.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')),
        "a scenario name is not lowercase-and-hyphen"
    );
}
