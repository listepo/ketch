/**
 * Public surface of @ketch/core: the shared domain types, the error type,
 * the source/platform/extractor seams, and the schema guards re-exported so
 * core callers have one import path.
 */

export { asciiLowercase, sanitizeComponent, validateRepo } from "@ketch/schemas";
// Namespaced, as in Rust: their generic names (install/uninstall, update,
// section) collide or would read wrong in the flat barrel.
export * as changelog from "./changelog.ts";
export * from "./config.ts";
export * from "./errors.ts";
export * from "./extract/index.ts";
export * from "./http.ts";
export * from "./install.ts";
export * from "./lockfile.ts";
export * from "./log.ts";
export * from "./manifest.ts";
export * from "./model.ts";
export * from "./platform/platform.ts";
export * from "./progress.ts";
export * as registry from "./registry.ts";
export * as selfupdate from "./selfupdate.ts";
export * as shell from "./shell.ts";
export * from "./source/github.ts";
export * from "./source/plugin.ts";
export * from "./source/registry.ts";
export * from "./source/source.ts";
export * from "./state.ts";
