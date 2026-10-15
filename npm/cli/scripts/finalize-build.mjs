import { mkdirSync, readFileSync, writeFileSync } from "node:fs";

mkdirSync("dist/cjs", { recursive: true });
writeFileSync("dist/cjs/package.json", '{"type":"commonjs"}\n');

const resolverPath = "dist/esm/resolver.js";
const resolver = readFileSync(resolverPath, "utf8");
writeFileSync(
  resolverPath,
  'import { fileURLToPath as __tuishotFileURLToPath } from "node:url";\n' +
    'const __filename = __tuishotFileURLToPath(import.meta.url);\n' +
    resolver,
);
