# ketch — todo

Empty: F1/F2/M3/F5 and the 2026-09-20 audit follow-ups (A2) are in `done.md`; the audit-13 evaluation (A3) also landed there with "already shipped, no gap" verdicts. The two remaining notes are not actionable from inside the repo:

- Notarisation secrets + first notarized release — creator step (needs the App Store Connect key).
- `src/config.rs` unit tests (`default_toml_*`, `ENV_GUARD`/`CleanEnv`) — uncommitted in the working tree, owned by an earlier session; commit separately, do not fold into an audit PR.
- B60. Windows self-update leaves `ketch.exe.old` behind
- B64. Binary name must be an explicit config parameter
- B65. Binary selection regression test
- R3. Cross-platform CI
- F8. Spinner and progress bar
- M9. `ketch list` refactor: `local`, `remote`, and both by default
- B66. Windows self-uninstall removes the registry entries ketch wrote at install
- B67. Uninstall deletes the package's install folder
- B68. Update installs into a fresh folder so stale files cannot interfere
- F9. `ketch install <pkg>` on an installed package offers the update
- B69. Uninstalling a package that is not installed prints only "not found"
- M11. Bash completion for every command
- M12. Windows completion: PowerShell `Register-ArgumentCompleter` and doskey macros for cmd
- F10. Coloured output: errors red, success green, warnings yellow
- F11. Emoji icons per operation, `emoji` config key (default true)
- R4. Fuzz testing with cargo-fuzz / libFuzzer
