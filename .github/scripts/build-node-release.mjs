import assert from "node:assert/strict";
import { copyFileSync, existsSync, readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { readTargets, repositoryRoot } from "../../npm/scripts/package-utils.mjs";

export function addonLibraryName(target) {
  if (target.includes("-apple-darwin")) return "libtuishot_node.dylib";
  if (target.includes("-pc-windows-msvc")) return "tuishot_node.dll";
  if (target.includes("-unknown-linux-")) return "libtuishot_node.so";
  throw new Error(`Unsupported addon target: ${target}`);
}

export function buildEnvironment(target, inherited) {
  const env = { ...inherited };
  // Node loads a shared object into its own musl process. Do not statically
  // link a second libc into the addon. Build this target inside Alpine, not
  // with Ubuntu's musl wrapper and its incompatible glibc GCC runtime.
  if (target.endsWith("-musl")) {
    if (env.CARGO_ENCODED_RUSTFLAGS !== undefined) {
      env.CARGO_ENCODED_RUSTFLAGS = [env.CARGO_ENCODED_RUSTFLAGS, "-C", "target-feature=-crt-static"]
        .filter(Boolean).join("\x1f");
    } else {
      env.RUSTFLAGS = `${env.RUSTFLAGS ?? ""} -C target-feature=-crt-static`.trim();
    }
  }
  return env;
}

function checked(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: repositoryRoot,
    stdio: "inherit",
    ...options,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`${command} exited with ${result.status ?? result.signal}`);
  }
  return result;
}

export function verifyAddon(addonPath) {
  const bindings = createRequire(import.meta.url)(addonPath);
  const options = { cols: 10, rows: 3 };
  const rendered = bindings.renderAnsi("\x1b[31mRed\x1b[0m\n界", options);
  assert.ok(Buffer.isBuffer(rendered.png));
  assert.deepEqual([...rendered.png.subarray(0, 8)], [137, 80, 78, 71, 13, 10, 26, 10]);
  assert.match(rendered.svg, /^<svg xmlns=/);
  assert.match(rendered.txt, /Red/);
  assert.equal(rendered.json.columns, 10);
  assert.equal(rendered.json.rows, 3);
  assert.equal(rendered.json.cells.length, 30);
  assert.deepEqual(bindings.renderGrid(rendered.json).png, rendered.png);
}

export function main(args) {
  if (args.length !== 2 || args[0] !== "--target") {
    throw new Error("Usage: node .github/scripts/build-node-release.mjs --target <rust-target>");
  }
  const target = readTargets().find((entry) => entry.target === args[1]);
  if (!target) throw new Error(`Unsupported addon target: ${args[1]}`);
  if (target.platform !== process.platform || target.arch !== process.arch) {
    throw new Error(`Run ${target.target} on a native ${target.platform}/${target.arch} runner so the artifact can be loaded and verified`);
  }
  const env = buildEnvironment(target.target, process.env);
  const protectedFiles = ["npm/node/index.cjs", "npm/node/index.js", "npm/node/index.d.ts"];
  const before = protectedFiles.map((file) => readFileSync(resolve(repositoryRoot, file)));
  // Committed JS/types are already tested. Cargo builds the napi-rs cdylib
  // directly, avoiding napi CLI's Windows filesystem-reconciliation failures.
  checked("cargo", ["build", "--locked", "--release", "--package", "tuishot-node", "--target", target.target], { env });
  const metadata = checked("cargo", ["metadata", "--locked", "--no-deps", "--format-version", "1"], { env, encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] });
  const targetDirectory = JSON.parse(metadata.stdout).target_directory;
  const library = resolve(targetDirectory, target.target, "release", addonLibraryName(target.target));
  if (!existsSync(library)) throw new Error(`Cargo did not produce ${library}`);
  checked(process.execPath, ["npm/scripts/generate-node-platform-package.mjs", "--target", target.target, "--addon", library]);
  const addonPath = resolve(repositoryRoot, "npm/node-platforms", target.target, target.addon);
  verifyAddon(addonPath);
  // Also exercise the unchanged JS loader and clean packed ESM/CJS consumers.
  // The local filename is the napi-rs loader's documented fallback for this target.
  const suffix = target.platform === "linux" ? `-${target.libc}` : target.platform === "win32" ? "-msvc" : "";
  const localAddon = resolve(repositoryRoot, "npm/node", `tuishot.${target.platform}-${target.arch}${suffix}.node`);
  copyFileSync(library, localAddon);
  checked(process.execPath, ["--test", "npm/node/test/native.test.mjs"]);
  checked(process.execPath, ["npm/scripts/node-integration.mjs"]);
  protectedFiles.forEach((file, index) => {
    assert.deepEqual(readFileSync(resolve(repositoryRoot, file)), before[index], `Build changed ${file}`);
  });
  console.log(`Verified native Node addon: ${target.nodePackage} (${process.platform}/${process.arch})`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2));
}
