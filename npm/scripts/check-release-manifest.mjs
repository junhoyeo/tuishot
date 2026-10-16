import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { cliTargets, readCargoVersion, readJson, readTargets, repositoryRoot } from "./package-utils.mjs";

// Guards the release against a silent drift between the target matrix in
// npm/targets.json and the package set .github/workflows/release.yml actually
// publishes. A mismatch means either a platform package is built and never
// published, or @tuishot/cli lists an optional dependency that will never exist.

const workflowPath = resolve(repositoryRoot, ".github/workflows/release.yml");
if (!existsSync(workflowPath)) {
  console.error(`missing ${workflowPath}`);
  process.exit(1);
}
const workflow = readFileSync(workflowPath, "utf8");
const version = readCargoVersion();
const targets = readTargets();
const cliBuildTargets = cliTargets(targets);
const errors = [];

function section(name) {
  const start = workflow.indexOf(`# release-manifest:${name}:begin`);
  const end = workflow.indexOf(`# release-manifest:${name}:end`);
  if (start === -1 || end === -1 || end < start) {
    errors.push(`release.yml is missing the "${name}" release-manifest block`);
    return undefined;
  }
  return workflow.slice(start, end);
}

/** Rust targets a build matrix block enumerates, in declaration order. */
function matrixTargets(name) {
  const block = section(name);
  if (block === undefined) return undefined;
  return [...block.matchAll(/^\s*- target: (\S+)$/gm)].map((match) => match[1]);
}

/** npm package names a publish block enumerates, in publish order. */
function publishedPackages(name) {
  const block = section(name);
  if (block === undefined) return undefined;
  return [...block.matchAll(/^\s*# - (@?[\w@/.-]+)$/gm)].map((match) => match[1]);
}

function compare(label, actual, wanted) {
  if (actual === undefined) return;
  if (JSON.stringify(actual) !== JSON.stringify(wanted)) {
    errors.push(`${label} mismatch:\n    release.yml:      ${actual.join(", ") || "(empty)"}\n    npm/targets.json: ${wanted.join(", ") || "(empty)"}`);
  }
}

compare("CLI build matrix targets", matrixTargets("cli-targets"), cliBuildTargets.map((target) => target.target));
compare("Node addon build matrix targets", matrixTargets("node-targets"), targets.map((target) => target.target));
compare(
  "CLI platform packages published",
  publishedPackages("cli-platform-packages"),
  cliBuildTargets.map((target) => target.package),
);
compare(
  "Node platform packages published",
  publishedPackages("node-platform-packages"),
  targets.map((target) => target.nodePackage),
);

// Every published platform package must be a real optional dependency of its
// parent package, so npm installs resolve the native artifact.
const cli = readJson(resolve(repositoryRoot, "npm/cli/package.json"));
const node = readJson(resolve(repositoryRoot, "npm/node/package.json"));
const wantedCliOptional = Object.fromEntries(cliBuildTargets.map((target) => [target.package, version]));
const wantedNodeOptional = Object.fromEntries(targets.map((target) => [target.nodePackage, version]));
if (JSON.stringify(cli.optionalDependencies) !== JSON.stringify(wantedCliOptional)) {
  errors.push("@tuishot/cli optionalDependencies do not match the CLI build targets");
}
if (JSON.stringify(node.optionalDependencies) !== JSON.stringify(wantedNodeOptional)) {
  errors.push("@tuishot/node optionalDependencies do not match the Node addon build targets");
}

// A target without a CLI build must leave no generated CLI package behind.
for (const target of targets) {
  const directory = resolve(repositoryRoot, `npm/platforms/${target.target}`);
  if (target.cli === false && existsSync(directory)) {
    errors.push(`${target.target} declares "cli": false but npm/platforms/${target.target} still exists`);
  }
  if (target.cli !== false && !existsSync(resolve(directory, "package.json"))) {
    errors.push(`missing generated manifest npm/platforms/${target.target}/package.json`);
  }
}

// The single source of truth for every published version is the Cargo workspace.
for (const [label, path] of [
  ["@tuishot/cli", "npm/cli/package.json"],
  ["@tuishot/node", "npm/node/package.json"],
  ["tuishot", "npm/tuishot/package.json"],
]) {
  const manifest = readJson(resolve(repositoryRoot, path));
  if (manifest.version !== version) {
    errors.push(`${label} is at ${manifest.version} but the Cargo workspace is at ${version}`);
  }
}

if (errors.length) {
  console.error(errors.map((error) => `- ${error}`).join("\n"));
  console.error("\nUpdate .github/workflows/release.yml and npm/targets.json together, then run `npm run sync:packages`.");
  process.exit(1);
}
console.log(
  `Release manifest matches npm/targets.json at ${version}: ` +
    `${cliBuildTargets.length} CLI targets, ${targets.length} Node addon targets, ` +
    `${cliBuildTargets.length + targets.length} platform packages.`,
);
