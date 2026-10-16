import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { brotliCompressSync, constants, gzipSync } from "node:zlib";
import { readCargoVersion, readJson, repositoryRoot } from "./package-utils.mjs";

const packageDirectory = resolve(repositoryRoot, "npm/wasm");
const reportPath = resolve(packageDirectory, "size-report.json");
const baselinePath = resolve(packageDirectory, "size-baseline.json");
const writeBaseline = process.argv.includes("--write-baseline");
const policy = { relativePercent: 15, absoluteBytes: 16 * 1024 };

function sizes(bytes) {
  return {
    raw: bytes.length,
    gzip9: gzipSync(bytes, { level: 9 }).length,
    brotli11: brotliCompressSync(bytes, {
      params: { [constants.BROTLI_PARAM_QUALITY]: 11 },
    }).length,
  };
}

function run(command, args, cwd = repositoryRoot) {
  const result = spawnSync(command, args, {
    cwd,
    encoding: "utf8",
    maxBuffer: 32 * 1024 * 1024,
    env: process.env,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} failed\n${result.stdout}\n${result.stderr}`);
  return result.stdout;
}

const temporary = mkdtempSync(join(tmpdir(), "tuishot-wasm-size-"));
try {
  const metadata = JSON.parse(
    run("npm", ["pack", "--json", "--ignore-scripts", "--pack-destination", temporary], packageDirectory),
  )[0];
  const wasm = readFileSync(resolve(packageDirectory, "dist/tuishot_wasm_bg.wasm"));
  const glue = readFileSync(resolve(packageDirectory, "dist/tuishot_wasm.js"));
  const loaderNames = ["runtime.js", "index.js", "browser.js", "node.js", "bun.js", "deno.js"];
  const loaders = Buffer.concat(loaderNames.map((name) => readFileSync(resolve(packageDirectory, name))));
  const tarball = readFileSync(resolve(temporary, metadata.filename));
  const font = readFileSync(resolve(repositoryRoot, "crates/tuishot-render/assets/JetBrainsMono-Regular.ttf"));
  const fontSha256 = createHash("sha256").update(font).digest("hex");
  const pinnedFontSha256 = "a0bf60ef0f83c5ed4d7a75d45838548b1f6873372dfac88f71804491898d138f";
  if (fontSha256 !== pinnedFontSha256) {
    throw new Error(`bundled font SHA-256 mismatch: ${fontSha256} != ${pinnedFontSha256}`);
  }
  const report = {
    schemaVersion: 1,
    packageVersion: readCargoVersion(),
    tools: { wasmBindgen: "0.2.129", binaryen: "132.0.0", optimization: "-Oz" },
    artifacts: {
      wasm: sizes(wasm),
      wasmBindgenGlue: sizes(glue),
      runtimeLoaders: sizes(loaders),
      packageTarball: sizes(tarball),
    },
    bundledFont: {
      ...sizes(font),
      sha256: fontSha256,
      note: "Included inside the wasm artifact; reported separately rather than double-counted.",
    },
  };
  writeFileSync(reportPath, `${JSON.stringify(report, null, 2)}\n`);

  if (writeBaseline) {
    writeFileSync(
      baselinePath,
      `${JSON.stringify({ ...report, regressionPolicy: policy }, null, 2)}\n`,
    );
    console.log(`Wrote measured wasm size baseline to ${baselinePath}.`);
  } else {
    const baseline = readJson(baselinePath);
    const failures = [];
    for (const [artifact, metrics] of Object.entries(report.artifacts)) {
      for (const [metric, actual] of Object.entries(metrics)) {
        const original = baseline.artifacts?.[artifact]?.[metric];
        if (!Number.isInteger(original)) {
          failures.push(`baseline is missing ${artifact}.${metric}`);
          continue;
        }
        const allowed = original + Math.max(policy.absoluteBytes, Math.ceil(original * policy.relativePercent / 100));
        if (actual > allowed) failures.push(`${artifact}.${metric}: ${actual} > allowed ${allowed} (baseline ${original})`);
      }
    }
    if (failures.length) {
      throw new Error(`wasm bundle size regression:\n${failures.map((failure) => `- ${failure}`).join("\n")}`);
    }
    console.log(`Wasm sizes are within ${policy.relativePercent}% or ${policy.absoluteBytes} bytes of baseline.`);
  }
  console.log(JSON.stringify(report, null, 2));
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
