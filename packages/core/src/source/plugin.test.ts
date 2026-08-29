/**
 * Port of the source/plugin.rs unit tests.
 *
 * Unix-only, matching the Rust suite's `#[cfg(unix)]` gate: a plugin is a
 * shell script here exactly as it is there. `wait_with_deadline` and
 * `capped` have no literal port (see plugin.ts's header) — the two tests
 * that exercised them directly are ported here as the same claims proved
 * against `runPlugin`, the real subprocess runner, instead of against an
 * in-memory stand-in.
 */

import fsp from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { PLUGIN_PREFIX, PROTOCOL_VERSION } from "@ketch/schemas";
import { afterEach, describe, expect, it } from "vitest";
import { PluginSource, runPlugin } from "./plugin.ts";
import { defaultListOpts } from "./source.ts";

const tmpDirs: string[] = [];

async function tempDir(): Promise<string> {
  const dir = await fsp.mkdtemp(path.join(os.tmpdir(), "ketch-plugin-test-"));
  tmpDirs.push(dir);
  return dir;
}

afterEach(async () => {
  await Promise.all(tmpDirs.splice(0).map((dir) => fsp.rm(dir, { recursive: true, force: true })));
});

/** A plugin is just an executable; the smallest honest one is a shell case. */
async function fakePlugin(dir: string, scheme: string, protocol: number): Promise<string> {
  const file = path.join(dir, `${PLUGIN_PREFIX}${scheme}`);
  await fsp.writeFile(
    file,
    `#!/bin/sh
case "$1" in
  capabilities) echo '{"protocol":${protocol},"scheme":"${scheme}","search":true}' ;;
  releases) echo '[{"tag":"v1.0.0","version":"1.0.0","assets":[]},
                   {"tag":"v2.0.0-rc1","version":"2.0.0-rc1","prerelease":true,"assets":[]},
                   {"tag":"v3.0.0","version":"3.0.0","draft":true,"assets":[]}]' ;;
  *) exit 1 ;;
esac
`,
  );
  await fsp.chmod(file, 0o755);
  return file;
}

describe.skipIf(process.platform === "win32")("runPlugin", () => {
  it("a plugin that never finishes is killed", async () => {
    const started = Date.now();
    await expect(runPlugin("/bin/sh", ["-c", "sleep 60"], { timeoutMs: 100 })).rejects.toThrow(
      /did not answer within/,
    );
    expect(Date.now() - started).toBeLessThan(5000);
  });

  it("a plugin cannot write without end", async () => {
    const started = Date.now();
    await expect(
      runPlugin("/bin/sh", ["-c", "yes x"], { maxOutputBytes: 10, timeoutMs: 5000 }),
    ).rejects.toThrow(/wrote more than 10 bytes/);
    expect(Date.now() - started).toBeLessThan(5000);
  });
});

describe.skipIf(process.platform === "win32")("PluginSource", () => {
  it("probes a plugin and filters what it returns", async () => {
    const dir = await tempDir();
    const file = await fakePlugin(dir, "demo", PROTOCOL_VERSION);

    const plugin = await PluginSource.probe(file);
    expect(plugin.scheme).toBe("demo");

    // Drafts always go, prereleases only when asked for.
    const stable = await plugin.listReleases("x/y", defaultListOpts());
    expect(stable).toHaveLength(1);
    expect(stable[0]?.tag).toBe("v1.0.0");

    const withPrerelease = await plugin.listReleases("x/y", {
      ...defaultListOpts(),
      includePrerelease: true,
    });
    expect(withPrerelease).toHaveLength(2);

    // Unsupported subcommands surface as errors, not as empty results.
    await expect(plugin.describe("x/y")).rejects.toThrow();
  });

  it("refuses a protocol it does not speak", async () => {
    const dir = await tempDir();
    const file = await fakePlugin(dir, "future", PROTOCOL_VERSION + 1);
    await expect(PluginSource.probe(file)).rejects.toThrow();
  });
});
