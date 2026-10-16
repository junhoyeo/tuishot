import assert from "node:assert/strict";
import { chmod, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { cliTargets, readCargoVersion, readJson, readTargets, repositoryRoot } from "./package-utils.mjs";

const temporary = await mkdtemp(join(tmpdir(), "tuishot-release-generation-"));
try {
  const fakeBinary = join(temporary, "input-binary");
  await writeFile(fakeBinary, "synthetic native artifact\n");
  await chmod(fakeBinary, 0o755);
  const targets = cliTargets(readTargets());
  assert.equal(targets.length, 6);
  const outputRoot = join(temporary, "platforms");
  for (const target of targets) {
    const generated = spawnSync(
      process.execPath,
      [
        resolve(repositoryRoot, "npm/scripts/generate-platform-package.mjs"),
        "--target",
        target.target,
        "--binary",
        fakeBinary,
        "--output-root",
        outputRoot,
      ],
      { cwd: repositoryRoot, encoding: "utf8" },
    );
    assert.equal(generated.status, 0, generated.stderr || generated.stdout);
    const directory = join(outputRoot, target.target);
    const manifest = readJson(join(directory, "package.json"));
    assert.equal(manifest.name, target.package);
    assert.equal(manifest.version, readCargoVersion());
    assert.deepEqual(manifest.engines, { node: ">=18" });
    assert.deepEqual(manifest.os, [target.platform]);
    assert.deepEqual(manifest.cpu, [target.arch]);
    assert.deepEqual(manifest.libc, target.npmLibc ? [target.npmLibc] : undefined);
    assert.equal(
      await readFile(join(directory, "bin", target.binary), "utf8"),
      "synthetic native artifact\n",
    );
    const packed = spawnSync("npm", ["pack", "--json", "--ignore-scripts"], {
      cwd: directory,
      encoding: "utf8",
      env: { ...process.env, npm_config_cache: join(temporary, "npm-cache") },
    });
    assert.equal(packed.status, 0, packed.stderr || packed.stdout);
    const result = JSON.parse(packed.stdout)[0];
    const files = result.files.map((file) => file.path).sort();
    assert.deepEqual(files, ["LICENSE-MIT", `bin/${target.binary}`, "package.json"]);
    assert.equal(await readFile(join(directory, "LICENSE-MIT"), "utf8"), await readFile(resolve(repositoryRoot, "LICENSE-MIT"), "utf8"));
  }
  console.log(`Generated and packed all ${targets.length} native packages at version ${readCargoVersion()}.`);
} finally {
  await rm(temporary, { recursive: true, force: true });
}
