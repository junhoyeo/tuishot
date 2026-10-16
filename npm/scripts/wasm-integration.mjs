import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  existsSync,
  mkdtempSync,
  mkdirSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { readCargoVersion, repositoryRoot } from "./package-utils.mjs";

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? repositoryRoot,
    encoding: "utf8",
    env: options.env ?? process.env,
    maxBuffer: 32 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed\n${result.stdout}\n${result.stderr}`);
  }
  return result;
}

const temporary = mkdtempSync(join(tmpdir(), "tuishot-wasm-packed-"));
try {
  const cache = join(temporary, "npm-cache");
  const consumer = join(temporary, "consumer");
  mkdirSync(cache, { recursive: true });
  mkdirSync(consumer, { recursive: true });
  const packed = run("npm", ["pack", "--json", "--ignore-scripts", "--pack-destination", temporary], {
    cwd: resolve(repositoryRoot, "npm/wasm"),
    env: { ...process.env, npm_config_cache: cache },
  });
  const metadata = JSON.parse(packed.stdout)[0];
  const files = metadata.files.map((file) => file.path).sort();
  assert.equal(files.filter((file) => file.endsWith(".wasm")).length, 1);
  for (const required of [
    "LICENSE-MIT",
    "OFL-JetBrainsMono.txt",
    "FONT-LICENSES.toml",
    "NOTICE",
    "dist/build-metadata.json",
    "dist/tuishot_wasm.js",
    "dist/tuishot_wasm_bg.wasm",
    "browser.js",
    "node.js",
    "bun.js",
    "deno.js",
    "index.d.ts",
  ]) {
    assert.ok(files.includes(required), `packed package is missing ${required}`);
  }

  const tarball = resolve(temporary, metadata.filename);
  writeFileSync(
    join(consumer, "package.json"),
    `${JSON.stringify({ name: "tuishot-wasm-consumer", private: true, type: "module" }, null, 2)}\n`,
  );
  run("npm", ["install", "--ignore-scripts", "--no-audit", "--no-fund", tarball], {
    cwd: consumer,
    env: { ...process.env, npm_config_cache: cache },
  });

  const script = join(consumer, "consumer.mjs");
  writeFileSync(
    script,
    `import assert from "node:assert/strict";\n` +
      `import root, { init, renderAnsi } from "@tuishot/wasm";\n` +
      `import * as nodeEntry from "@tuishot/wasm/node";\n` +
      `for (const path of ["@tuishot/wasm/browser", "@tuishot/wasm/bun", "@tuishot/wasm/deno"]) { const entry = await import(path); assert.equal(typeof entry.init, "function"); }\n` +
      `await init(); assert.equal(root.init, init); assert.equal(nodeEntry.version(), ${JSON.stringify(readCargoVersion())});\n` +
      `const result = await renderAnsi("\\x1b[31mPACKED\\x1b[0m\\n界", { cols: 10, rows: 3 });\n` +
      `assert.ok(result.png instanceof Uint8Array); assert.deepEqual([...result.png.subarray(0, 8)], [137,80,78,71,13,10,26,10]);\n` +
      `assert.match(result.svg, /^<svg/); assert.equal(result.json.cells.length, 30); assert.equal(result.txt.split("\\n").length, 3);\n`,
  );
  run(process.execPath, [script], { cwd: consumer });

  const installed = resolve(consumer, "node_modules/@tuishot/wasm");
  assert.ok(existsSync(installed));
  assert.deepEqual(
    readdirSync(resolve(installed, "dist")).filter((file) => file.endsWith(".wasm")),
    ["tuishot_wasm_bg.wasm"],
  );
  console.log(`Packed @tuishot/wasm consumer passed on Node ${process.versions.node}.`);
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
