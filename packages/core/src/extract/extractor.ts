/**
 * Turning a downloaded file into a directory of files.
 *
 * Extractors are selected by sniffing content, not by trusting the file name:
 * release assets are routinely named `.tar.gz` while being a plain binary, or
 * `.zip` while being a tarball. This module owns the interface; the formats
 * themselves live beside it.
 *
 * The member-path guard (`safeMemberPath`) is implemented once in
 * @ketch/schemas — manifest validation needs it too and schemas must not
 * import core — and re-exported here, where extractor code reaches for it.
 */

/**
 * Reject archive member paths that would write outside the destination —
 * the guard against "zip slip" / tar traversal. A member named
 * `../../../.zshrc` must never be honoured, no matter who published it.
 */
export { safeMemberPath } from "@ketch/schemas";

/** One archive format. */
export interface Extractor {
  /** Stable identifier used in logs and `--verbose` output. */
  readonly id: string;

  /**
   * Can this extractor handle the file? `head` is the first 512 bytes,
   * already read, so implementations do not each re-open the file.
   */
  detect(path: string, head: Uint8Array): boolean;

  /** Unpack `src` into the existing directory `dest`. */
  extract(src: string, dest: string): Promise<void>;
}
