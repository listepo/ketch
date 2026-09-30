# Ideas

- `glob_preferred` falls back to directory order when a `bin` glob matches several files and none matches the entry's `name` (or the entry has no `name`). Same class of bug as B64 (binary choice depends on the OS); found while fixing B64, left out of its scope.
