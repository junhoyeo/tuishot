import assert from "node:assert/strict";
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { restoreCliArtifacts } from "./restore-cli-artifacts.mjs";
import { cliTargets, platformManifest, readCargoVersion, readTargets } from "../../npm/scripts/package-utils.mjs";

test("artifact transport strips executable bits; restoration fixes both files and packed modes", {skip: process.platform === "win32"}, () => {
  const temporary = mkdtempSync(join(tmpdir(), "tuishot-artifact-mode-"));
  try {
    const targets = cliTargets(readTargets());
    const version = readCargoVersion();
    const incoming = join(temporary, "incoming");
    const output = join(temporary, "platforms");
    for (const target of targets) {
      const source = join(incoming, `cli-platform-${target.target}`);
      mkdirSync(join(source, "bin"), {recursive:true});
      writeFileSync(join(source,"package.json"),JSON.stringify(platformManifest(target,version)));
      const binary=join(source,"bin",target.binary);
      writeFileSync(binary,"#!/bin/sh\nexit 0\n");
      chmodSync(binary,0o644);
      assert.equal(statSync(binary).mode & 0o111,0);
    }
    restoreCliArtifacts(incoming,output);
    for (const target of targets) {
      const directory=join(output,target.target);
      assert.equal(statSync(join(directory,"bin",target.binary)).mode & 0o777,0o755);
      assert.equal(readFileSync(join(directory,"bin",target.binary),"utf8"),"#!/bin/sh\nexit 0\n");
      const packed=spawnSync("npm",["pack","--dry-run","--json","--ignore-scripts"],{cwd:directory,encoding:"utf8"});
      assert.equal(packed.status,0,packed.stderr);
      const file=JSON.parse(packed.stdout)[0].files.find(entry=>entry.path===`bin/${target.binary}`);
      assert.equal(file.mode & 0o777,0o755,"npm tarball must preserve the executable bit");
    }
  } finally {
    rmSync(temporary,{recursive:true,force:true});
  }
});
