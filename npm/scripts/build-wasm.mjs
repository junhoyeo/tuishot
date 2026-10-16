import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, renameSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readCargoVersion, repositoryRoot } from "./package-utils.mjs";

const WASM_BINDGEN_VERSION = "0.2.129";
const BINARYEN_VERSION = "132.0.0";
const target = "wasm32-unknown-unknown";
const packageDirectory = resolve(repositoryRoot, "npm/wasm");
const outputDirectory = resolve(packageDirectory, "dist");
const targetDirectory = process.env.CARGO_TARGET_DIR
  ? resolve(process.env.CARGO_TARGET_DIR)
  : resolve(repositoryRoot, "target");

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: repositoryRoot,
    encoding: "utf8",
    env: { ...process.env, CARGO_INCREMENTAL: "0" },
    maxBuffer: 32 * 1024 * 1024,
    ...options,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(
      `${command} ${args.join(" ")} failed with ${result.status}\n${result.stdout ?? ""}${result.stderr ?? ""}`,
    );
  }
  return result;
}

const bindgen = "wasm-bindgen";
const bindgenVersion = run(bindgen, ["--version"]).stdout.trim();
if (bindgenVersion !== `wasm-bindgen ${WASM_BINDGEN_VERSION}`) {
  throw new Error(
    `wasm-bindgen CLI must be exactly ${WASM_BINDGEN_VERSION}; got ${JSON.stringify(bindgenVersion)}`,
  );
}

run(process.execPath, [resolve(repositoryRoot, "npm/scripts/check-wasm-graph.mjs")]);
run("cargo", ["build", "--locked", "--release", "--target", target, "-p", "tuishot-wasm"]);

rmSync(outputDirectory, { recursive: true, force: true });
mkdirSync(outputDirectory, { recursive: true });
const input = resolve(targetDirectory, target, "release/tuishot_wasm.wasm");
if (!existsSync(input)) throw new Error(`Cargo did not produce ${input}`);
run(bindgen, [
  input,
  "--target", "web",
  "--out-dir", outputDirectory,
  "--out-name", "tuishot_wasm",
  "--no-typescript",
]);

const binaryenManifest = resolve(repositoryRoot, "node_modules/binaryen/package.json");
if (!existsSync(binaryenManifest)) {
  throw new Error(`pinned binaryen ${BINARYEN_VERSION} is missing; run npm ci before building wasm`);
}
const binaryen = JSON.parse(readFileSync(binaryenManifest, "utf8"));
if (binaryen.version !== BINARYEN_VERSION) {
  throw new Error(`binaryen must be exactly ${BINARYEN_VERSION}; got ${JSON.stringify(binaryen.version)}`);
}
const wasmOpt = resolve(
  repositoryRoot,
  "node_modules/.bin",
  process.platform === "win32" ? "wasm-opt.cmd" : "wasm-opt",
);
if (!existsSync(wasmOpt)) throw new Error(`pinned wasm-opt binary is missing at ${wasmOpt}`);
const wasm = resolve(outputDirectory, "tuishot_wasm_bg.wasm");
const optimized = `${wasm}.optimized`;
run(wasmOpt, ["-Oz", wasm, "-o", optimized]);
renameSync(optimized, wasm);
console.log(`Optimized wasm with pinned binaryen ${BINARYEN_VERSION} (-Oz).`);

const glue = resolve(outputDirectory, "tuishot_wasm.js");
const digest = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");
writeFileSync(
  resolve(outputDirectory, "build-metadata.json"),
  `${JSON.stringify({
    schemaVersion: 1,
    packageVersion: readCargoVersion(),
    wasmBindgen: WASM_BINDGEN_VERSION,
    binaryen: BINARYEN_VERSION,
    optimization: "-Oz",
    wasmSha256: digest(wasm),
    glueSha256: digest(glue),
  }, null, 2)}\n`,
);

const wasmAssets = readdirSync(outputDirectory).filter((entry) => entry.endsWith(".wasm"));
if (JSON.stringify(wasmAssets) !== JSON.stringify(["tuishot_wasm_bg.wasm"])) {
  throw new Error(`expected exactly one wasm asset, found: ${wasmAssets.join(", ")}`);
}
console.log(`Built @tuishot/wasm with wasm-bindgen ${WASM_BINDGEN_VERSION}.`);
