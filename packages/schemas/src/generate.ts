/**
 * JSON Schema generation: one `schemas/<name>.schema.json` per file format
 * ketch reads or writes.
 *
 * The generated files are committed and published; every runtime JSON file
 * ketch writes carries a `$schema` field pointing at their raw GitHub URL, so
 * editors validate and complete them. Run with `bun run src/generate.ts` (or
 * `node src/generate.ts`) after changing a schema, and commit the result.
 */

import { mkdirSync, writeFileSync } from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";
import { z } from "zod";
import { configFileSchema } from "./config.ts";
import { lockfileSchema } from "./lockfile.ts";
import { manifestSchema } from "./manifest.ts";
import { registryPackageSchema } from "./registry.ts";
import { stateSchema } from "./state.ts";
import { schemaUrl } from "./util.ts";

/** Every generated document, keyed by the file name it is written under. */
export function schemaDocuments(): Record<string, Record<string, unknown>> {
  const formats: Record<string, z.ZodType> = {
    config: configFileSchema,
    manifest: manifestSchema,
    lockfile: lockfileSchema,
    state: stateSchema,
    registry: registryPackageSchema,
  };
  const documents: Record<string, Record<string, unknown>> = {};
  for (const [name, schema] of Object.entries(formats)) {
    // Input semantics: defaulted fields stay optional and only strict
    // objects are closed, which is the contract for a file a person edits.
    documents[name] = { $id: schemaUrl(name), ...z.toJSONSchema(schema, { io: "input" }) };
  }
  return documents;
}

function main(): void {
  const dir = fileURLToPath(new URL("../schemas/", import.meta.url));
  mkdirSync(dir, { recursive: true });
  for (const [name, document] of Object.entries(schemaDocuments())) {
    writeFileSync(`${dir}${name}.schema.json`, `${JSON.stringify(document, null, 2)}\n`);
  }
}

// Only write when run as a script, so tests can import schemaDocuments()
// without touching the working tree.
if (process.argv[1] !== undefined && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main();
}
