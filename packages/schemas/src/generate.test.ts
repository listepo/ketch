/** The committed JSON Schemas must be valid JSON and in sync with the source. */

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { schemaDocuments } from "./generate.ts";

const dir = fileURLToPath(new URL("../schemas/", import.meta.url));

describe("schemaDocuments", () => {
  it("every format renders to valid JSON with its published $id", () => {
    for (const [name, document] of Object.entries(schemaDocuments())) {
      const decoded: unknown = JSON.parse(JSON.stringify(document));
      expect(decoded).toEqual(document);
      expect(document["$id"]).toBe(
        `https://raw.githubusercontent.com/listepo/ketch/main/packages/schemas/schemas/${name}.schema.json`,
      );
    }
  });

  it("the committed schema files are in sync with the zod schemas", () => {
    for (const [name, document] of Object.entries(schemaDocuments())) {
      const committed: unknown = JSON.parse(readFileSync(`${dir}${name}.schema.json`, "utf8"));
      expect(committed, `${name}.schema.json is stale; run \`bun run generate\``).toEqual(document);
    }
  });
});
