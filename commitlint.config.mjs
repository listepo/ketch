//! Conventional Commit rules shared by local hooks and CI.

// Commit format is conventional commits, exactly as config-conventional
// defines them: the default type enum already covers every type this repo
// uses, and release-plz reads the same `feat:`/`fix:`/`!`/`BREAKING CHANGE:`
// grammar to decide the version bump. Local rules would drift from both, so
// there are none.
export default { extends: ["@commitlint/config-conventional"] };
