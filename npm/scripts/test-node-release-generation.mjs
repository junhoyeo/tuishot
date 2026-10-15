import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { readCargoVersion, readJson, readTargets, repositoryRoot } from "./package-utils.mjs";

const temporary = await mkdtemp(join(tmpdir(), "tuishot-node-release-generation-"));
try {
  const fakeAddon = join(temporary, "input.node");
  await writeFile(fakeAddon, "synthetic Node-API artifact\n");
  const targets = readTargets();
  assert.equal(targets.length, 8);
  assert.deepEqual(targets.map((target) => target.nodePackage), [
    "@tuishot/node-darwin-arm64",
    "@tuishot/node-darwin-x64",
    "@tuishot/node-linux-x64-gnu",
    "@tuishot/node-linux-x64-musl",
    "@tuishot/node-linux-arm64-gnu",
    "@tuishot/node-linux-arm64-musl",
    "@tuishot/node-win32-x64-msvc",
    "@tuishot/node-win32-arm64-msvc",
  ]);
  const outputRoot = join(temporary, "node-platforms");
  for (const target of targets) {
    const generated = spawnSync(
      process.execPath,
      [
        resolve(repositoryRoot, "npm/scripts/generate-node-platform-package.mjs"),
        "--target", target.target,
        "--addon", fakeAddon,
        "--output-root", outputRoot,
      ],
      { cwd: repositoryRoot, encoding: "utf8" },
    );
    assert.equal(generated.status, 0, generated.stderr || generated.stdout);
    const directory = join(outputRoot, target.target);
    const manifest = readJson(join(directory, "package.json"));
    assert.equal(manifest.name, target.nodePackage);
    assert.equal(manifest.version, readCargoVersion());
    assert.deepEqual(manifest.engines, { node: ">=18" });
    assert.deepEqual(manifest.os, [target.platform]);
    assert.deepEqual(manifest.cpu, [target.arch]);
    assert.deepEqual(manifest.libc, target.npmLibc ? [target.npmLibc] : undefined);
    assert.equal(manifest.main, `./${target.addon}`);
    assert.equal(await readFile(join(directory, target.addon), "utf8"), "synthetic Node-API artifact\n");
    assert.equal(
      await readFile(join(directory, "LICENSE-MIT"), "utf8"),
      await readFile(resolve(repositoryRoot, "LICENSE-MIT"), "utf8"),
    );
    const packed = spawnSync("npm", ["pack", "--json", "--ignore-scripts"], {
      cwd: directory,
      encoding: "utf8",
      env: { ...process.env, npm_config_cache: join(temporary, "npm-cache") },
    });
    assert.equal(packed.status, 0, packed.stderr || packed.stdout);
    assert.deepEqual(
      JSON.parse(packed.stdout)[0].files.map((file) => file.path).sort(),
      ["LICENSE-MIT", "package.json", target.addon],
    );
  }
  console.log(`Generated and packed all ${targets.length} @tuishot/node native packages at ${readCargoVersion()}.`);
} finally {
  await rm(temporary, { recursive: true, force: true });
}
