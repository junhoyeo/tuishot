import { existsSync, readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// Publishes one group of npm packages in a fixed order. The order matters:
// a platform package must exist on the registry before the package that
// declares it as an optional dependency, and @tuishot/cli must exist before
// the unscoped tuishot alias that depends on it.

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function option(name) {
  const index = process.argv.indexOf(`--${name}`);
  return index === -1 ? undefined : process.argv[index + 1];
}

const kind = option("kind");
const version = option("version");
if (!kind || !version) {
  console.error("Usage: node .github/scripts/publish-packages.mjs --kind <group> --version <version>");
  process.exit(2);
}

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

async function importTargets() {
  const module = await import(resolve(repositoryRoot, "npm/scripts/package-utils.mjs"));
  const targets = module.readTargets();
  return { targets, cliTargets: module.cliTargets(targets) };
}

const { targets, cliTargets } = await importTargets();

/** Each group lists directories to publish, in publish order. */
const groups = {
  "cli-platforms": cliTargets.map((target) => `npm/platforms/${target.target}`),
  cli: ["npm/cli", "npm/tuishot"],
  "node-platforms": targets.map((target) => `npm/node-platforms/${target.target}`),
  node: ["npm/node"],
  wasm: ["npm/wasm"],
};

const directories = groups[kind];
if (!directories) {
  console.error(`Unknown group ${kind}; choose one of ${Object.keys(groups).join(", ")}`);
  process.exit(2);
}

// OIDC trusted publishing signs a provenance attestation. The token fallback
// cannot, so it publishes without one.
const useToken = process.env.TUISHOT_NPM_AUTH === "token";
const publishArgs = ["publish", "--access", "public"];
if (!useToken) publishArgs.push("--provenance");

let published = 0;
for (const directory of directories) {
  const absolute = resolve(repositoryRoot, directory);
  const manifestPath = resolve(absolute, "package.json");
  if (!existsSync(manifestPath)) {
    console.error(`missing ${directory}/package.json`);
    process.exit(1);
  }
  const manifest = readJson(manifestPath);
  if (manifest.version !== version) {
    console.error(`${manifest.name} is at ${manifest.version} but this release is ${version}`);
    process.exit(1);
  }
  if (manifest.private) {
    console.error(`${manifest.name} is private and must not be published`);
    process.exit(1);
  }
  console.log(`\n==> npm ${publishArgs.join(" ")} (${manifest.name}@${manifest.version} from ${directory})`);
  const result = spawnSync("npm", publishArgs, { cwd: absolute, stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    console.error(`npm publish failed for ${manifest.name} with exit code ${result.status}`);
    process.exit(result.status ?? 1);
  }
  published += 1;
}

console.log(`\nPublished ${published} package(s) for group ${kind} at ${version}.`);
