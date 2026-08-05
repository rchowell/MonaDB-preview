import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import openapiTS, { astToString } from "openapi-typescript";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const openapiPath = join(root, "tsp-output", "@typespec", "openapi3", "openapi.json");
const outDir = join(root, "generated");
const outFile = join(outDir, "openapi.ts");

const ast = await openapiTS(new URL(`file://${openapiPath}`));
await mkdir(outDir, { recursive: true });
await writeFile(outFile, astToString(ast));

// Also copy OpenAPI next to generated for consumers.
const openapi = await readFile(openapiPath, "utf8");
await writeFile(join(outDir, "openapi.json"), openapi);

console.log(`Wrote ${outFile}`);
