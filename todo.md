# ketch — todo

Empty: F1/F2/M3/F5 and the 2026-09-20 audit follow-ups (A2) are in `done.md`; the audit-13 evaluation (A3) also landed there with "already shipped, no gap" verdicts. The two remaining notes are not actionable from inside the repo:

- Notarisation secrets + first notarized release — creator step (needs the App Store Connect key).
- `src/config.rs` unit tests (`default_toml_*`, `ENV_GUARD`/`CleanEnv`) — uncommitted in the working tree, owned by an earlier session; commit separately, do not fold into an audit PR.
- B60. Windows self-update leaves `ketch.exe.old` behind
- B61. `self update` swap fails on a transient Windows lock
