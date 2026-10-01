# ketch — todo

Empty: F1/F2/M3/F5 and the 2026-09-20 audit follow-ups (A2) are in `done.md`; the audit-13 evaluation (A3) also landed there with "already shipped, no gap" verdicts. The two remaining notes are not actionable from inside the repo:

- Notarisation secrets + first notarized release — creator step (needs the App Store Connect key).
- `src/config.rs` unit tests (`default_toml_*`, `ENV_GUARD`/`CleanEnv`) — uncommitted in the working tree, owned by an earlier session; commit separately, do not fold into an audit PR.
- B60. Windows self-update leaves `ketch.exe.old` behind
- B64. Binary name must be an explicit config parameter
- B65. Binary selection regression test
- B71. Ambiguous bin glob refuses instead of taking directory order
- R3. Cross-platform CI
- F8. Spinner and progress bar
- M9. `ketch list` refactor: `local`, `remote`, and both by default
- R4. Fuzz testing with cargo-fuzz / libFuzzer
- M14. JSON Schema for the package manifest
- M15. `log_level` and `log_format` as enums in `config.toml`
- M16. One module owns config file I/O
- R5. Workspace split: `ketch-core` library crate
- R6. A reporter instead of the global `ui::` sink
- R7. Decisions out of the pipeline
- R8. Core calls from a long-running host
- R9. `ketch-ffi`: the core exported through UniFFI
- R10. Toolkit choice for the Windows and Linux desktop apps
- F12. Native macOS app (SwiftUI) on `ketch-ffi`
- F13. macOS app release pipeline
- F14. Design system for the macOS app: `DESIGN.md` and tokens
