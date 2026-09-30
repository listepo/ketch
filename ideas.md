# Ideas

- `glob_preferred` falls back to directory order when a `bin` glob matches several files and none matches the entry's `name` (or the entry has no `name`). Same class of bug as B64 (binary choice depends on the OS); found while fixing B64, left out of its scope.
- JSON Schema for the package manifest (`ketch.toml`, `Manifest` in `src/model.rs`), the third TOML file ketch owns, the same way M13 did `config.toml` and `ketch.lock`. Its nested types and custom (de)serializers make it larger than M13.
- `log_level` and `log_format` in `config.toml` are free strings checked at load time; as serde enums the schema would list the allowed values (`AGENTS.md`: constraints live in the types).
