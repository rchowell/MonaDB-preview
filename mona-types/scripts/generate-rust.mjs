import { spawnSync } from "node:child_process";
import { access } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const openapiPath = join(root, "generated", "openapi.json");
const apiDir = join(root, "..", "mona-api");

await access(openapiPath);

const result = spawnSync(
  "cargo",
  [
    "run",
    "--quiet",
    "--features",
    "generate",
    "--bin",
    "generate_models",
    "--",
    openapiPath,
  ],
  {
    cwd: apiDir,
    stdio: "inherit",
    env: process.env,
  },
);

if (result.error) {
  console.error(result.error.message);
  process.exit(1);
}

if (result.status !== 0) {
  process.exit(result.status ?? 1);
}
