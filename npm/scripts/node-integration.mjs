import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { readCargoVersion, readJson, readTargets, repositoryRoot } from "./package-utils.mjs";

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? repositoryRoot,
    encoding: "utf8",
    env: options.env ?? process.env,
    maxBuffer: 16 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  if (result.status !== 0 && !options.allowFailure) {
    throw new Error(
      `${command} ${args.join(" ")} failed with ${result.status}${result.signal ? `/${result.signal}` : ""}\n` +
        `stdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
    );
  }
  return result;
}

function pack(directory, cache) {
  const result = run("npm", ["pack", "--json", "--ignore-scripts"], {
    cwd: directory,
    env: { ...process.env, npm_config_cache: cache },
  });
  const parsed = JSON.parse(result.stdout);
  assert.equal(parsed.length, 1);
  return {
    tarball: join(directory, parsed[0].filename),
    files: parsed[0].files.map((file) => file.path).sort(),
  };
}

function writeJson(path, value) {
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
}

const targets = readTargets();
const libc = process.platform === "linux"
  ? process.report?.getReport()?.header?.glibcVersionRuntime
    ? "gnu"
    : "musl"
  : undefined;
const host = targets.find(
  (target) =>
    target.platform === process.platform &&
    target.arch === process.arch &&
    (target.platform !== "linux" || target.libc === libc),
);
if (!host) throw new Error(`Unsupported packed-test host ${process.platform}/${process.arch}/${libc ?? "none"}`);

const localAddons = readdirSync(resolve(repositoryRoot, "npm/node"))
  .filter((entry) => entry.endsWith(".node"))
  .map((entry) => resolve(repositoryRoot, "npm/node", entry));
assert.equal(localAddons.length, 1, "build @tuishot/node once for the current host before packed integration");
const localAddon = localAddons[0];
const version = readCargoVersion();
const temporary = mkdtempSync(join(tmpdir(), "tuishot-node-packed-"));

try {
  const cache = join(temporary, "npm-cache");
  const stage = join(temporary, "stage");
  const platformRoot = join(stage, "node-platforms");
  const consumer = join(temporary, "consumer");
  mkdirSync(cache, { recursive: true });
  mkdirSync(stage, { recursive: true });
  mkdirSync(consumer, { recursive: true });

  run(process.execPath, [
    resolve(repositoryRoot, "npm/scripts/generate-node-platform-package.mjs"),
    "--target", host.target,
    "--addon", localAddon,
    "--output-root", platformRoot,
  ]);
  const platformDirectory = join(platformRoot, host.target);
  const platformPack = pack(platformDirectory, cache);
  assert.deepEqual(platformPack.files, ["LICENSE-MIT", "package.json", host.addon]);

  const nodeStage = join(stage, "node");
  cpSync(resolve(repositoryRoot, "npm/node"), nodeStage, { recursive: true });
  for (const entry of readdirSync(nodeStage)) {
    if (entry.endsWith(".node") || entry === "bindings.generated.d.ts" || entry === "test") {
      rmSync(join(nodeStage, entry), { recursive: true, force: true });
    }
  }
  const nodeManifestPath = join(nodeStage, "package.json");
  const nodeManifest = readJson(nodeManifestPath);
  nodeManifest.optionalDependencies = { [host.nodePackage]: `file:${platformPack.tarball}` };
  writeJson(nodeManifestPath, nodeManifest);
  const nodePack = pack(nodeStage, cache);
  assert.deepEqual(nodePack.files, [
    "LICENSE-MIT",
    "README.md",
    "index.cjs",
    "index.d.ts",
    "index.js",
    "package.json",
  ]);

  writeJson(join(consumer, "package.json"), {
    name: "tuishot-node-packed-consumer",
    private: true,
    type: "module",
  });
  run("npm", ["install", "--ignore-scripts", "--no-audit", "--no-fund", nodePack.tarball], {
    cwd: consumer,
    env: { ...process.env, npm_config_cache: cache },
  });

  const installedNode = join(consumer, "node_modules", "@tuishot", "node");
  const installedPlatform = join(consumer, "node_modules", ...host.nodePackage.split("/"));
  assert.ok(existsSync(installedNode));
  assert.ok(existsSync(join(installedNode, "LICENSE-MIT")));
  assert.ok(existsSync(join(installedPlatform, host.addon)));
  assert.ok(existsSync(join(installedPlatform, "LICENSE-MIT")));
  assert.equal(readJson(join(installedNode, "package.json")).version, version);
  assert.equal(readJson(join(installedPlatform, "package.json")).version, version);

  const esm = join(consumer, "esm.mjs");
  writeFileSync(
    esm,
    `import assert from "node:assert/strict";\n` +
      `import { renderAnsi, renderGrid } from "@tuishot/node";\n` +
      `const first = renderAnsi("\\x1b[31mPACKED\\x1b[0m\\n界", { cols: 12, rows: 3 });\n` +
      `assert.ok(Buffer.isBuffer(first.png)); assert.deepEqual([...first.png.subarray(0,8)], [137,80,78,71,13,10,26,10]);\n` +
      `assert.match(first.svg, /^<svg/); assert.match(first.txt, /^PACKED/); assert.equal(first.json.cells.length, 36);\n` +
      `const second = renderGrid(first.json); assert.deepEqual(second.png, first.png); assert.equal(second.svg, first.svg); assert.equal(second.txt, first.txt); assert.deepEqual(second.json, first.json);\n`,
  );
  run(process.execPath, [esm], { cwd: consumer });

  const cjs = join(consumer, "cjs.cjs");
  writeFileSync(
    cjs,
    `const assert = require("node:assert/strict");\n` +
      `const { renderAnsi, renderGrid } = require("@tuishot/node");\n` +
      `assert.equal(typeof renderAnsi, "function"); assert.equal(typeof renderGrid, "function");\n` +
      `assert.match(renderAnsi("CJS", { cols: 4, rows: 1, formats: ["txt"] }).txt, /^CJS/);\n`,
  );
  run(process.execPath, [cjs], { cwd: consumer });

  rmSync(installedPlatform, { recursive: true, force: true });
  const missing = run(process.execPath, ["-e", 'require("@tuishot/node")'], {
    cwd: consumer,
    allowFailure: true,
  });
  assert.notEqual(missing.status, 0, "loading without the optional package must fail");
  assert.match(missing.stderr, /Cannot find native binding/);

  console.log(`Packed @tuishot/node consumer passed on Node ${process.versions.node} for ${host.nodePackage}.`);
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
