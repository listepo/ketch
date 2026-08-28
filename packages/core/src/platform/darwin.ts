/**
 * The macOS backend: asset scoring for darwin targets, placement into the
 * store, symlinks and `.app` handling, codesign/Gatekeeper trust checks, and
 * quarantine stripping.
 *
 * Placeholder — the port of `src/platform/macos.rs` lands here.
 */

import type { Platform } from "./platform.ts";

export function createDarwinPlatform(): Platform {
  throw new Error("not yet ported");
}
