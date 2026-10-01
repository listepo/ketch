# Ideas

- `BinSpec` (`bin = [{...}]` entries) and `AssetSelector` (`[asset]`) lack `#[serde(deny_unknown_fields)]`, so a misspelt key there is silently ignored, contradicting `docs/MANIFESTS.md` ("unknown keys are an error"). Found by M14. Fixing it changes behaviour (a manifest with a typo stops loading), so it would be a breaking change. Needs creator approval before it becomes a task.
