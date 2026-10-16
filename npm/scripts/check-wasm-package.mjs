import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { resolve } from "node:path";
import { readCargoVersion, readJson, repositoryRoot } from "./package-utils.mjs";

const directory = resolve(repositoryRoot, "npm/wasm");
const manifest = readJson(resolve(directory, "package.json"));
assert.equal(manifest.name, "@tuishot/wasm");
assert.equal(manifest.version, readCargoVersion());
assert.equal(manifest.type, "module");
assert.deepEqual(manifest.engines, { node: ">=18" });
assert.equal(
  readFileSync(resolve(directory, "LICENSE-MIT"), "utf8"),
  readFileSync(resolve(repositoryRoot, "LICENSE-MIT"), "utf8"),
);
assert.equal(
  readFileSync(resolve(directory, "OFL-JetBrainsMono.txt"), "utf8"),
  readFileSync(resolve(repositoryRoot, "crates/tuishot-render/assets/OFL-JetBrainsMono.txt"), "utf8"),
);
assert.equal(
  readFileSync(resolve(directory, "FONT-LICENSES.toml"), "utf8"),
  readFileSync(resolve(repositoryRoot, "crates/tuishot-render/assets/FONT-LICENSES.toml"), "utf8"),
);
for (const path of [".", "./browser", "./node", "./bun", "./deno"]) {
  assert.ok(manifest.exports[path], `missing export ${path}`);
}
const dist = resolve(directory, "dist");
assert.ok(existsSync(resolve(dist, "tuishot_wasm.js")), "build wasm-bindgen glue before packing");
assert.deepEqual(
  readdirSync(dist).filter((entry) => entry.endsWith(".wasm")),
  ["tuishot_wasm_bg.wasm"],
  "the npm package must contain one wasm binary",
);
const wasmPath = resolve(dist, "tuishot_wasm_bg.wasm");
const gluePath = resolve(dist, "tuishot_wasm.js");
assert.match(readFileSync(gluePath, "utf8"), /tuishot_wasm_bg\.wasm/);
const metadata = readJson(resolve(dist, "build-metadata.json"));
assert.deepEqual(
  {
    schemaVersion: metadata.schemaVersion,
    packageVersion: metadata.packageVersion,
    wasmBindgen: metadata.wasmBindgen,
    binaryen: metadata.binaryen,
    optimization: metadata.optimization,
  },
  {
    schemaVersion: 1,
    packageVersion: readCargoVersion(),
    wasmBindgen: "0.2.129",
    binaryen: "132.0.0",
    optimization: "-Oz",
  },
  "built wasm metadata is stale or was produced by unpinned tools",
);
const sha256 = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");
assert.equal(metadata.wasmSha256, sha256(wasmPath), "built wasm hash does not match metadata");
assert.equal(metadata.glueSha256, sha256(gluePath), "generated glue hash does not match metadata");
for (const file of manifest.files) {
  assert.ok(existsSync(resolve(directory, file)), `package file is missing: ${file}`);
}
console.log("Checked @tuishot/wasm exports, single wasm asset, version, and licenses.");
