/**
 * Public surface of @ketch/core: the shared domain types, the error type,
 * the source/platform/extractor seams, and the schema guards re-exported so
 * core callers have one import path.
 */

export { asciiLowercase, sanitizeComponent, validateRepo } from "@ketch/schemas";
export * from "./config.ts";
export * from "./errors.ts";
export * from "./extract/extractor.ts";
export * from "./log.ts";
export * from "./model.ts";
export * from "./platform/platform.ts";
export * from "./progress.ts";
export * from "./source/source.ts";
export * from "./state.ts";
