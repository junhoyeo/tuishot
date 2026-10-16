import { chmodSync, copyFileSync, existsSync, mkdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { basename, resolve } from "node:path";
import {
  canonicalJson,
  platformManifest,
  readCargoVersion,
  readTargets,
  repositoryRoot,
} from "./package-utils.mjs";

function option(name) {
  const index = process.argv.indexOf(name);
  return index === -1 ? undefined : process.argv[index + 1];
}

const targetName = option("--target");
const binaryPath = option("--binary");
const outputRoot = resolve(option("--output-root") ?? resolve(repositoryRoot, "npm/platforms"));
if (!targetName || !binaryPath) {
  console.error(
    "Usage: node npm/scripts/generate-platform-package.mjs --target <rust-target> --binary <path> [--output-root <path>]",
  );
  process.exit(2);
}
const target = readTargets().find((candidate) => candidate.target === targetName);
if (!target) {
  console.error(`Unknown target ${targetName}; choose a target from npm/targets.json`);
  process.exit(2);
}
if (target.cli === false) {
  console.error(`${targetName} declares "cli": false in npm/targets.json and ships no @tuishot/cli package`);
  process.exit(2);
}
const sourceBinary = resolve(binaryPath);
if (!existsSync(sourceBinary) || !statSync(sourceBinary).isFile()) {
  console.error(`Native binary does not exist or is not a file: ${sourceBinary}`);
  process.exit(2);
}
if (basename(sourceBinary) === "package.json") {
  console.error(`Refusing to use a manifest as a native binary: ${sourceBinary}`);
  process.exit(2);
}

const directory = resolve(outputRoot, target.target);
const destination = resolve(directory, "bin", target.binary);
rmSync(directory, { recursive: true, force: true });
mkdirSync(resolve(directory, "bin"), { recursive: true });
copyFileSync(sourceBinary, destination);
copyFileSync(resolve(repositoryRoot, "LICENSE-MIT"), resolve(directory, "LICENSE-MIT"));
if (target.platform !== "win32") chmodSync(destination, 0o755);
writeFileSync(resolve(directory, "package.json"), canonicalJson(platformManifest(target, readCargoVersion())));
console.log(directory);
