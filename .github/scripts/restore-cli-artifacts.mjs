import assert from "node:assert/strict";
import { lstatSync } from "node:fs";
import { resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { cliTargets, readCargoVersion, readJson, readTargets, repositoryRoot } from "../../npm/scripts/package-utils.mjs";

export function restoreCliArtifacts(inputRoot, outputRoot = resolve(repositoryRoot, "npm/platforms")) {
  const version = readCargoVersion();
  for (const target of cliTargets(readTargets())) {
    const source = resolve(inputRoot, `cli-platform-${target.target}`);
    const manifest = readJson(resolve(source, "package.json"));
    assert.equal(manifest.name, target.package);
    assert.equal(manifest.version, version);
    const binary = resolve(source, "bin", target.binary);
    assert.ok(lstatSync(binary).isFile(), `Artifact binary must be a regular file: ${binary}`);
    assert.notEqual(source, resolve(outputRoot, target.target), "Artifact input must be separate from generated output");
    // download-artifact resets every regular file to 0644. Regenerate the
    // package after downloading so the binary's 0755 mode reaches npm pack.
    const generated = spawnSync(process.execPath, [
      resolve(repositoryRoot, "npm/scripts/generate-platform-package.mjs"),
      "--target", target.target, "--binary", binary, "--output-root", outputRoot,
    ], { cwd: repositoryRoot, stdio: "inherit" });
    if (generated.error) throw generated.error;
    assert.equal(generated.status, 0, `Failed to restore ${target.package}`);
    const restored = resolve(outputRoot, target.target, "bin", target.binary);
    assert.equal(lstatSync(restored).mode & 0o777, 0o755, `Non-executable release binary: ${restored}`);
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.argv.length !== 3) throw new Error("Usage: restore-cli-artifacts.mjs <download-directory>");
  restoreCliArtifacts(resolve(process.argv[2]));
}
