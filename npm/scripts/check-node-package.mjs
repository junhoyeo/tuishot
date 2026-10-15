import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { readCargoVersion, readJson, readTargets, repositoryRoot } from "./package-utils.mjs";

const directory = resolve(repositoryRoot, "npm/node");
const manifest = readJson(resolve(directory, "package.json"));
const targets = readTargets();
const expectedOptional = Object.fromEntries(targets.map((target) => [target.nodePackage, readCargoVersion()]));
assert.equal(manifest.name, "@tuishot/node");
assert.equal(manifest.version, readCargoVersion());
assert.deepEqual(manifest.engines, { node: ">=18" });
assert.deepEqual(manifest.optionalDependencies, expectedOptional);
assert.equal(
  readFileSync(resolve(directory, "LICENSE-MIT"), "utf8"),
  readFileSync(resolve(repositoryRoot, "LICENSE-MIT"), "utf8"),
);
const loader = readFileSync(resolve(directory, "index.cjs"), "utf8");
const embeddedVersions = [...loader.matchAll(/bindingPackageVersion !== '([^']+)'/g)].map((match) => match[1]);
assert.ok(embeddedVersions.length > 0, "standard napi-rs loader must include package-version checks");
assert.ok(embeddedVersions.every((embedded) => embedded === readCargoVersion()), "loader version is out of sync");
for (const target of targets) {
  assert.match(loader, new RegExp(target.nodePackage.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
}
console.log("Checked @tuishot/node source package and exact optional package matrix.");
