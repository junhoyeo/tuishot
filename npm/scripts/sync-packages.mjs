import { existsSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { spawnSync } from "node:child_process";
import {
  canonicalJson,
  generatedTargetsTypeScript,
  nodePlatformManifest,
  platformManifest,
  readCargoVersion,
  readJson,
  readTargets,
  repositoryRoot,
} from "./package-utils.mjs";

const check = process.argv.includes("--check");
const version = readCargoVersion();
const targets = readTargets();
const license = readFileSync(resolve(repositoryRoot, "LICENSE-MIT"), "utf8");
const rootPath = resolve(repositoryRoot, "package.json");
const cliPath = resolve(repositoryRoot, "npm/cli/package.json");
const aliasPath = resolve(repositoryRoot, "npm/tuishot/package.json");
const nodePath = resolve(repositoryRoot, "npm/node/package.json");
const nodeLoaderPath = resolve(repositoryRoot, "npm/node/index.cjs");
const generatedTargetsPath = resolve(repositoryRoot, "npm/cli/src/targets.generated.ts");
const platformsRoot = resolve(repositoryRoot, "npm/platforms");
const nodePlatformsRoot = resolve(repositoryRoot, "npm/node-platforms");
const expectedNodePlatformDirs = new Set(targets.map((target) => target.target));
const expectedPlatformDirs = new Set(targets.map((target) => target.target));
const errors = [];
const rootOptionalDependencies = Object.fromEntries([
  ...targets.map((target) => [target.package, `file:npm/platforms/${target.target}`]),
  ...targets.map((target) => [target.nodePackage, `file:npm/node-platforms/${target.target}`]),
]);

function expectFile(path, wanted) {
  if (check) {
    if (!existsSync(path)) {
      errors.push(`missing generated file ${path}`);
      return;
    }
    const actual = readFileSync(path, "utf8");
    if (actual !== wanted) {
      errors.push(`out of sync: ${path}`);
    }
  } else {
    writeFileSync(path, wanted);
  }
}

const root = readJson(rootPath);
root.optionalDependencies = rootOptionalDependencies;
expectFile(rootPath, canonicalJson(root));

const cli = readJson(cliPath);
cli.version = version;
cli.optionalDependencies = Object.fromEntries(targets.map((target) => [target.package, version]));
expectFile(cliPath, canonicalJson(cli));

const node = readJson(nodePath);
node.version = version;
node.optionalDependencies = Object.fromEntries(targets.map((target) => [target.nodePackage, version]));
expectFile(nodePath, canonicalJson(node));
if (!existsSync(nodeLoaderPath)) {
  errors.push(`missing generated file ${nodeLoaderPath}`);
} else {
  const loader = readFileSync(nodeLoaderPath, "utf8")
    .replace(/(bindingPackageVersion !== ')[^']+(')/g, `$1${version}$2`)
    .replace(/(binding package version mismatch, expected )[^ ]+( but got)/gi, `$1${version}$2`);
  expectFile(nodeLoaderPath, loader);
}

const alias = readJson(aliasPath);
alias.version = version;
alias.dependencies = { "@tuishot/cli": version };
expectFile(aliasPath, canonicalJson(alias));
expectFile(resolve(repositoryRoot, "npm/cli/LICENSE-MIT"), license);
expectFile(resolve(repositoryRoot, "npm/tuishot/LICENSE-MIT"), license);
expectFile(resolve(repositoryRoot, "npm/node/LICENSE-MIT"), license);
expectFile(generatedTargetsPath, generatedTargetsTypeScript(targets));

for (const target of targets) {
  const directory = resolve(platformsRoot, target.target);
  if (!check) {
    // Release generation is deterministic. Only generated package directories live here.
    rmSync(directory, { recursive: true, force: true });
    const { mkdirSync } = await import("node:fs");
    mkdirSync(resolve(directory, "bin"), { recursive: true });
  }
  expectFile(resolve(directory, "package.json"), canonicalJson(platformManifest(target, version)));
  expectFile(resolve(directory, "LICENSE-MIT"), license);

  const nodeDirectory = resolve(nodePlatformsRoot, target.target);
  if (!check) {
    rmSync(nodeDirectory, { recursive: true, force: true });
    const { mkdirSync } = await import("node:fs");
    mkdirSync(nodeDirectory, { recursive: true });
  }
  expectFile(resolve(nodeDirectory, "package.json"), canonicalJson(nodePlatformManifest(target, version)));
  expectFile(resolve(nodeDirectory, "LICENSE-MIT"), license);
}

if (existsSync(platformsRoot)) {
  const { readdirSync, statSync } = await import("node:fs");
  for (const entry of readdirSync(platformsRoot)) {
    if (statSync(resolve(platformsRoot, entry)).isDirectory() && !expectedPlatformDirs.has(entry)) {
      if (check) errors.push(`unexpected platform directory: npm/platforms/${entry}`);
      else rmSync(resolve(platformsRoot, entry), { recursive: true, force: true });
    }
  }
}

if (existsSync(nodePlatformsRoot)) {
  const { readdirSync, statSync } = await import("node:fs");
  for (const entry of readdirSync(nodePlatformsRoot)) {
    if (statSync(resolve(nodePlatformsRoot, entry)).isDirectory() && !expectedNodePlatformDirs.has(entry)) {
      if (check) errors.push(`unexpected platform directory: npm/node-platforms/${entry}`);
      else rmSync(resolve(nodePlatformsRoot, entry), { recursive: true, force: true });
    }
  }
}

if (!check) {
  const lock = spawnSync("npm", ["install", "--package-lock-only", "--ignore-scripts"], {
    cwd: repositoryRoot,
    stdio: "inherit",
  });
  if (lock.error) throw lock.error;
  if (lock.status !== 0) process.exit(lock.status ?? 1);
} else {
  const lock = spawnSync("npm", ["ci", "--ignore-scripts", "--omit=optional", "--dry-run"], {
    cwd: repositoryRoot,
    encoding: "utf8",
  });
  if (lock.error) errors.push(`could not verify package-lock.json: ${lock.error.message}`);
  if (lock.status !== 0) {
    errors.push(`package-lock.json is not installable:\n${lock.stderr || lock.stdout}`);
  } else if (!existsSync(resolve(repositoryRoot, "package-lock.json"))) {
    errors.push("missing package-lock.json");
  } else {
    const lockfile = readJson(resolve(repositoryRoot, "package-lock.json"));
    const root = lockfile.packages?.[""];
    const cliLock = lockfile.packages?.["npm/cli"];
    const nodeLock = lockfile.packages?.["npm/node"];
    const aliasLock = lockfile.packages?.["npm/tuishot"];
    const wantedOptional = Object.fromEntries(targets.map((target) => [target.package, version]));
    const wantedNodeOptional = Object.fromEntries(targets.map((target) => [target.nodePackage, version]));
    const sortedEntries = (value = {}) => Object.entries(value).sort(([left], [right]) => left.localeCompare(right));
    if (
      root?.name !== "tuishot-workspace" ||
      JSON.stringify(sortedEntries(root?.optionalDependencies)) !==
        JSON.stringify(sortedEntries(rootOptionalDependencies))
    ) {
      errors.push("package-lock root is out of sync");
    }
    if (
      cliLock?.version !== version ||
      JSON.stringify(sortedEntries(cliLock?.optionalDependencies)) !== JSON.stringify(sortedEntries(wantedOptional))
    ) {
      errors.push("package-lock @tuishot/cli entry is out of sync");
    }
    if (
      nodeLock?.version !== version ||
      JSON.stringify(sortedEntries(nodeLock?.optionalDependencies)) !== JSON.stringify(sortedEntries(wantedNodeOptional))
    ) {
      errors.push("package-lock @tuishot/node entry is out of sync");
    }
    if (aliasLock?.version !== version || aliasLock.dependencies?.["@tuishot/cli"] !== version) {
      errors.push("package-lock tuishot entry is out of sync");
    }
    for (const target of targets) {
      const platformPath = `npm/platforms/${target.target}`;
      const platformLock = lockfile.packages?.[platformPath];
      const linkLock = lockfile.packages?.[`node_modules/${target.package}`];
      if (
        platformLock?.name !== target.package ||
        platformLock?.version !== version ||
        platformLock?.optional !== true ||
        JSON.stringify(platformLock?.os) !== JSON.stringify([target.platform]) ||
        JSON.stringify(platformLock?.cpu) !== JSON.stringify([target.arch]) ||
        JSON.stringify(platformLock?.libc) !== JSON.stringify(target.npmLibc ? [target.npmLibc] : undefined) ||
        linkLock?.resolved !== platformPath ||
        linkLock?.link !== true
      ) {
        errors.push(`package-lock ${target.package} entries are out of sync`);
      }

      const nodePlatformPath = `npm/node-platforms/${target.target}`;
      const nodePlatformLock = lockfile.packages?.[nodePlatformPath];
      const nodeLinkLock = lockfile.packages?.[`node_modules/${target.nodePackage}`];
      if (
        nodePlatformLock?.name !== target.nodePackage ||
        nodePlatformLock?.version !== version ||
        nodePlatformLock?.optional !== true ||
        JSON.stringify(nodePlatformLock?.os) !== JSON.stringify([target.platform]) ||
        JSON.stringify(nodePlatformLock?.cpu) !== JSON.stringify([target.arch]) ||
        JSON.stringify(nodePlatformLock?.libc) !== JSON.stringify(target.npmLibc ? [target.npmLibc] : undefined) ||
        nodeLinkLock?.resolved !== nodePlatformPath ||
        nodeLinkLock?.link !== true
      ) {
        errors.push(`package-lock ${target.nodePackage} entries are out of sync`);
      }
    }
  }
}

if (errors.length) {
  console.error(errors.map((error) => `- ${error}`).join("\n"));
  console.error("Run `npm run sync:packages` and commit the generated changes.");
  process.exit(1);
}
console.log(`${check ? "Checked" : "Synchronized"} npm packages at Cargo workspace version ${version}.`);
