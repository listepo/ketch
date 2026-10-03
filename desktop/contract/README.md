# Contract scenarios

Language-neutral JSON scenarios that every app's fake core replays, so the
macOS, Windows and Linux fakes speak the records, event streams and errors the
real core does and cannot drift from it or from each other.

The files in `scenarios/` are generated, never edited. They are built from the
Rust values `ketch-ffi` hands a front end, in
`crates/ketch-ffi/tests/contract.rs`. That test fails when a file is stale or
has no generator; regenerate with:

```bash
KETCH_BLESS=1 cargo nextest run -p ketch-ffi contract
```

## A scenario

```json
{
  "name": "install-binary-choice",
  "description": "What the scenario shows.",
  "call": { "operation": "install", "specs": ["uv"], "options": { "...": "..." } },
  "script": [ { "type": "event", "event": { "type": "step", "package": "uv", "stage": "resolving" } } ],
  "outcome": { "type": "ok", "value": [ "..." ] }
}
```

- `call`: the operation and the arguments the app passes. `operation` is one
  of `installed`, `search`, `outdated`, `install`, `upgrade`, `uninstall`,
  `changelog_range`, `doctor`.
- `script`: what happens during the call, in order. A `type: "event"` step is
  something the core reports (`step`, `status`, `success`, `warn`, `note`,
  `debug`, `began`, `sized`, `progress`, `ended`, `abandoned`). A
  `type: "ask"` step is a question the core puts to the app (`choose_binary`,
  `stop_processes`); the `answer` shows what a person answered when the
  scenario was written, and a fake lets its test's decider answer instead.
- `outcome`: `type: "ok"` with the returned `value`, whose shape is the call's
  return type in `ketch-ffi` (`Vec<Package>`, `SearchResults`, `Vec<Upgrade>`,
  `Vec<Installed>`, `Vec<Changelog>`, `Vec<Check>`), or `type: "error"` with a
  `KetchError` (`busy` with an optional `pid`, `cancelled`, `not_found`,
  `network`, `verification`, `other`).

Field names are the Rust field names, in snake case. `Option` fields are
`null` when absent. Task ids in events are unique within a scenario.

## What a fake does with one

A fake core reads the file for the call it is asked to make. A read returns
the `value`. A mutating call plays the `script` in order, handing events to
the reporter and questions to the decider, stops with `Cancelled` if the caller
cancels between steps, and then returns the `value` or throws the `error`.
`Busy` shows a banner with Retry, `Cancelled` is logged and not shown as an
alert, and every other error is an alert with the core's wording.

The macOS app's fake is `desktop/macos/Ketch/Core/FakeKetchCore.swift`
(decoding in `ContractScenario.swift`), tested by
`desktop/macos/KetchTests/ContractScenarioTests.swift`.

The Windows app's fake is `desktop/windows/Ketch.AppCore/FakeKetchCore.cs`
(decoding in `ContractScenario.cs`), tested by
`desktop/windows/Ketch.AppCore.Tests`. The app copies the scenarios beside its
binary, and Settings can play one for the next install, upgrade or uninstall.
The Linux fake reads the same directory when that app exists.
