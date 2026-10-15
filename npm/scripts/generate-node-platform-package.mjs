import { copyFileSync, existsSync, mkdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import {
  canonicalJson,
  nodePlatformManifest,
  readCargoVersion,
  readTargets,
  repositoryRoot,
} from "./package-utils.mjs";

function option(name) {
  const index = process.argv.indexOf(name);
  return index === -1 ? undefined : process.argv[index + 1];
}

const targetName = option("--target");
const addonPath = option("--addon");
const outputRoot = resolve(option("--output-root") ?? resolve(repositoryRoot, "npm/node-platforms"));
if (!targetName || !addonPath) {
  console.error(
    "Usage: node npm/scripts/generate-node-platform-package.mjs --target <rust-target> --addon <path> [--output-root <path>]",
  );
  process.exit(2);
}
const target = readTargets().find((candidate) => candidate.target === targetName);
if (!target) {
  console.error(`Unknown target ${targetName}; choose a target from npm/targets.json`);
  process.exit(2);
}
const sourceAddon = resolve(addonPath);
if (!existsSync(sourceAddon) || !statSync(sourceAddon).isFile()) {
  console.error(`Native addon does not exist or is not a file: ${sourceAddon}`);
  process.exit(2);
}

const directory = resolve(outputRoot, target.target);
rmSync(directory, { recursive: true, force: true });
mkdirSync(directory, { recursive: true });
copyFileSync(sourceAddon, resolve(directory, target.addon));
copyFileSync(resolve(repositoryRoot, "LICENSE-MIT"), resolve(directory, "LICENSE-MIT"));
writeFileSync(
  resolve(directory, "package.json"),
  canonicalJson(nodePlatformManifest(target, readCargoVersion())),
);
console.log(directory);
