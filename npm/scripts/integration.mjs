import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  chmodSync,
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, delimiter, join, resolve } from "node:path";
import { detectLibc } from "../cli/dist/esm/resolver.js";
import { readCargoVersion, readJson, readTargets, repositoryRoot } from "./package-utils.mjs";

const version = readCargoVersion();
const targets = readTargets();
const libc = process.platform === "linux" ? detectLibc() : undefined;
const host = targets.find(
  (target) =>
    target.platform === process.platform &&
    target.arch === process.arch &&
    (target.platform !== "linux" || target.libc === libc),
);
if (!host) {
  throw new Error(`Packed integration does not support host ${process.platform}/${process.arch}/${libc ?? "none"}`);
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? repositoryRoot,
    encoding: "utf8",
    env: options.env ?? process.env,
    maxBuffer: 16 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(
      `${command} ${args.join(" ")} failed with ${result.status}${result.signal ? `/${result.signal}` : ""}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
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

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

const targetDirectory = resolve(
  process.env.CARGO_TARGET_DIR ?? resolve(repositoryRoot, "target"),
);
const nativeBinary = join(targetDirectory, "debug", process.platform === "win32" ? "tuishot.exe" : "tuishot");
run("cargo", ["build", "--locked", "-p", "tuishot"]);
assert.ok(existsSync(nativeBinary), `missing locally built binary ${nativeBinary}`);

const temporary = mkdtempSync(join(tmpdir(), "tuishot-packed-integration-"));
try {
  const cache = join(temporary, "npm-cache");
  const stage = join(temporary, "stage");
  const platforms = join(stage, "platforms");
  const consumer = join(temporary, "consumer");
  mkdirSync(cache, { recursive: true });
  mkdirSync(stage, { recursive: true });
  mkdirSync(consumer, { recursive: true });

  run(
    process.execPath,
    [
      resolve(repositoryRoot, "npm/scripts/generate-platform-package.mjs"),
      "--target",
      host.target,
      "--binary",
      nativeBinary,
      "--output-root",
      platforms,
    ],
    { cwd: repositoryRoot },
  );
  const platformDirectory = join(platforms, host.target);
  const platformPack = pack(platformDirectory, cache);
  const platformTarball = platformPack.tarball;
  assert.deepEqual(platformPack.files, ["LICENSE-MIT", `bin/${host.binary}`, "package.json"]);

  const cliStage = join(stage, "cli");
  cpSync(resolve(repositoryRoot, "npm/cli"), cliStage, { recursive: true });
  const cliManifestPath = join(cliStage, "package.json");
  const cliManifest = readJson(cliManifestPath);
  // Edit only the throwaway staging copy. The committed exact eight-package
  // manifest stays untouched while this offline install receives one host tarball.
  cliManifest.optionalDependencies = { [host.package]: `file:${platformTarball}` };
  writeJson(cliManifestPath, cliManifest);
  const cliPack = pack(cliStage, cache);
  const cliTarball = cliPack.tarball;
  assert.ok(cliPack.files.includes("LICENSE-MIT"), "scoped CLI tarball must carry the MIT license");

  const aliasStage = join(stage, "tuishot");
  cpSync(resolve(repositoryRoot, "npm/tuishot"), aliasStage, { recursive: true });
  const aliasManifestPath = join(aliasStage, "package.json");
  const aliasManifest = readJson(aliasManifestPath);
  aliasManifest.dependencies = { "@tuishot/cli": `file:${cliTarball}` };
  writeJson(aliasManifestPath, aliasManifest);
  const aliasPack = pack(aliasStage, cache);
  const aliasTarball = aliasPack.tarball;
  assert.ok(aliasPack.files.includes("LICENSE-MIT"), "alias tarball must carry the MIT license");

  writeJson(join(consumer, "package.json"), {
    name: "tuishot-packed-consumer",
    private: true,
    type: "module",
  });
  run(
    "npm",
    ["install", "--ignore-scripts", "--no-audit", "--no-fund", aliasTarball],
    {
      cwd: consumer,
      env: { ...process.env, npm_config_cache: cache },
    },
  );

  const installedCli = join(consumer, "node_modules", "@tuishot", "cli");
  const installedAlias = join(consumer, "node_modules", "tuishot");
  const installedPlatform = join(consumer, "node_modules", ...host.package.split("/"));
  for (const path of [installedCli, installedAlias, installedPlatform, join(installedPlatform, "bin", host.binary)]) {
    assert.ok(existsSync(path), `expected installed artifact ${path}`);
  }
  assert.equal(readJson(join(installedCli, "package.json")).version, version);
  assert.equal(readJson(join(installedAlias, "package.json")).version, version);
  assert.equal(readJson(join(installedPlatform, "package.json")).version, version);

  const importChecks = [
    [
      "import-scoped.mjs",
      'import { shoot, runScenes } from "@tuishot/cli";\nif (typeof shoot !== "function" || typeof runScenes !== "function") process.exit(7);\n',
    ],
    [
      "import-alias.mjs",
      'import { shoot, runScenes } from "tuishot";\nif (typeof shoot !== "function" || typeof runScenes !== "function") process.exit(7);\n',
    ],
    [
      "require-scoped.cjs",
      'const { shoot, runScenes } = require("@tuishot/cli");\nif (typeof shoot !== "function" || typeof runScenes !== "function") process.exit(7);\n',
    ],
    [
      "require-alias.cjs",
      'const { shoot, runScenes } = require("tuishot");\nif (typeof shoot !== "function" || typeof runScenes !== "function") process.exit(7);\n',
    ],
  ];
  for (const [name, source] of importChecks) {
    const path = join(consumer, name);
    writeFileSync(path, source);
    run(process.execPath, [path], { cwd: consumer });
  }

  const capturedCwd = join(consumer, "captured-cwd");
  const capturedHome = join(consumer, "captured-home");
  mkdirSync(capturedCwd);
  mkdirSync(capturedHome);
  const apiCheck = join(consumer, "api-check.mjs");
  const captureChild = 'const fs = require("node:fs"); const line = ["A=" + process.argv[1], "E=" + process.env.VALUE, "C=" + (fs.realpathSync(process.cwd()) === fs.realpathSync(process.env.EXPECTED_CWD)), "H=" + (process.env.HOME === process.env.EXPECTED_HOME), "L=" + (process.env.SHOULD_NOT_LEAK ?? "clear")].join(" "); process.stdout.write(line); setTimeout(() => {}, 500)';
  writeFileSync(
    apiCheck,
    `import assert from "node:assert/strict";\n` +
      `import { shoot } from "@tuishot/cli";\n` +
      `process.env.SHOULD_NOT_LEAK = "parent-value";\n` +
      `const cwd = ${JSON.stringify(capturedCwd)};\n` +
      `const home = ${JSON.stringify(capturedHome)};\n` +
      `const child = ${JSON.stringify(captureChild)};\n` +
      `const result = await shoot({\n` +
      `  command: process.execPath,\n` +
      `  args: ["-e", child, "ARGUMENT_OK"],\n` +
      `  size: "120x6",\n` +
      `  waitText: "L=clear",\n` +
      `  waitStable: "100ms",\n` +
      `  envClear: true,\n` +
      `  env: { VALUE: "present", EXPECTED_CWD: cwd, EXPECTED_HOME: home },\n` +
      `  cwd, home, formats: ["png", "svg", "txt", "json"], scale: 1, chrome: "none", timeout: "10s",\n` +
      `});\n` +
      `assert.deepEqual(Object.keys(result).sort(), ["json", "png", "svg", "txt"]);\n` +
      `assert.ok(Buffer.isBuffer(result.png));\n` +
      `assert.deepEqual([...result.png.subarray(0, 8)], [137,80,78,71,13,10,26,10]);\n` +
      `assert.match(result.svg, /^<svg/);\n` +
      `assert.match(result.txt, /A=ARGUMENT_OK E=present C=true H=true L=clear/);\n` +
      `const grid = result.json;\n` +
      `assert.equal(grid.columns, 120); assert.equal(grid.rows, 6); assert.equal(grid.cells.length, 720);\n` +
      `assert.ok(grid.cursor === null || (Number.isInteger(grid.cursor.x) && Number.isInteger(grid.cursor.y)));\n` +
      `for (const cell of grid.cells) {\n` +
      `  assert.equal(typeof cell.symbol, "string"); assert.ok(["single","wide","continuation"].includes(cell.width));\n` +
      `  assert.ok(cell.style && cell.style.attributes); assert.equal(typeof cell.style.attributes.bold, "boolean");\n` +
      `  assert.ok(["default","indexed","rgb"].includes(cell.style.foreground.kind));\n` +
      `}\n`,
  );
  run(process.execPath, [apiCheck], { cwd: consumer });

  const scenePath = join(consumer, "scene.toml");
  const sceneOutput = join(consumer, "scene-output");
  const sceneChild = "process.stdout.write('SCENE_READY')";
  writeFileSync(
    scenePath,
    `version = 1\n\n[[scene]]\nname = "packed-api"\ncommand = [${JSON.stringify(process.execPath)}, "-e", ${JSON.stringify(sceneChild)}]\nsize = "40x4"\ntimeout = "10s"\nenv_clear = true\ndeterministic_fonts = true\noutput_dir = "scene-output"\n\n[[scene.step]]\nwait_text = "SCENE_READY"\n\n[[scene.step]]\nshot = "scene"\nformats = ["txt", "json"]\n`,
  );
  const sceneCheck = join(consumer, "scene-check.cjs");
  writeFileSync(
    sceneCheck,
    `const assert = require("node:assert/strict");\n` +
      `const fs = require("node:fs");\n` +
      `const { NativeCommandError, runScenes } = require("tuishot");\n` +
      `(async () => {\n` +
      `  const value = await runScenes("./scene.toml"); assert.equal(value, undefined);\n` +
      `  assert.match(fs.readFileSync(${JSON.stringify(join(sceneOutput, "scene.txt"))}, "utf8"), /SCENE_READY/);\n` +
      `  const grid = JSON.parse(fs.readFileSync(${JSON.stringify(join(sceneOutput, "scene.json"))}, "utf8")); assert.equal(grid.columns, 40); assert.equal(grid.rows, 4);\n` +
      `  const failure = await runScenes("./missing-scene.toml").then(() => null, (error) => error);\n` +
      `  assert.ok(failure instanceof NativeCommandError); assert.equal(failure.code, 1); assert.equal(failure.signal, null); assert.match(failure.stderr, /failed to read scene file/);\n` +
      `})().catch((error) => { console.error(error); process.exitCode = 1; });\n`,
  );
  run(process.execPath, [sceneCheck], { cwd: consumer });

  const cliBin = join(installedCli, "bin", "tuishot.js");
  const aliasBin = join(installedAlias, "bin", "tuishot.js");
  for (const bin of [cliBin, aliasBin]) {
    const result = run(process.execPath, [bin, "--version"], { cwd: consumer });
    assert.equal(result.stdout.trim(), `tuishot ${version}`);
  }

  // Remove every installed copy of the optional package before introducing a
  // fake PATH binary. A successful marker write would prove an unsafe fallback.
  const nativePackageName = host.package.split("/")[1];
  const nativeDirectories = [];
  function visit(directory) {
    if (!existsSync(directory)) return;
    for (const entry of readdirSync(directory)) {
      const path = join(directory, entry);
      let stat;
      try { stat = statSync(path); } catch { continue; }
      if (!stat.isDirectory()) continue;
      if (entry === nativePackageName && basename(directory) === "@tuishot") nativeDirectories.push(path);
      else visit(path);
    }
  }
  visit(join(consumer, "node_modules"));
  assert.ok(nativeDirectories.length > 0, "native package was not found before deletion test");
  for (const directory of nativeDirectories) rmSync(directory, { recursive: true, force: true });

  const fakePath = join(temporary, "fake-path");
  const marker = join(temporary, "PATH_FALLBACK_EXECUTED");
  mkdirSync(fakePath);
  const fakeBinary = join(fakePath, process.platform === "win32" ? "tuishot.cmd" : "tuishot");
  if (process.platform === "win32") {
    writeFileSync(fakeBinary, `@echo off\r\necho unsafe>${marker}\r\n`);
  } else {
    writeFileSync(fakeBinary, `#!/bin/sh\nprintf unsafe > ${JSON.stringify(marker)}\n`);
    chmodSync(fakeBinary, 0o755);
  }
  const missing = spawnSync(process.execPath, [aliasBin, "--version"], {
    cwd: consumer,
    encoding: "utf8",
    env: { ...process.env, PATH: `${fakePath}${delimiter}${process.env.PATH ?? ""}` },
  });
  assert.notEqual(missing.status, 0, "launcher unexpectedly succeeded without its native package");
  assert.match(missing.stderr, new RegExp(`${escapeRegExp(host.package)} is not installed`));
  assert.equal(existsSync(marker), false, "launcher fell back to a PATH binary");
  assert.doesNotMatch(`${missing.stdout}\n${missing.stderr}`, /unsafe/);

  console.log(
    `Packed integration passed for ${host.target}: platform, scoped CLI, alias, ESM/CJS, API, scenes, bins, and no PATH fallback.`,
  );
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
