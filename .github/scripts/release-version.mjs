import { readCargoVersion } from "../../npm/scripts/package-utils.mjs";

// Resolves the one version this release publishes. The Cargo workspace version
// is the single source of truth: a v* tag must match it exactly, so a release
// can never ship npm packages and crates at different versions.

function option(name) {
  const index = process.argv.indexOf(`--${name}`);
  return index === -1 ? undefined : process.argv[index + 1];
}

const refType = option("ref-type") ?? "";
const refName = option("ref-name") ?? "";
const dryRun = option("dry-run") === "true";
const version = readCargoVersion();

if (refType === "tag") {
  if (!refName.startsWith("v")) {
    console.error(`tag ${refName} does not start with "v"; release tags look like v${version}`);
    process.exit(1);
  }
  if (refName.slice(1) !== version) {
    console.error(`tag ${refName} does not match the Cargo workspace version ${version}`);
    console.error("Bump [workspace.package].version, run `npm run sync:packages`, then retag.");
    process.exit(1);
  }
  // dry_run wins even on a tag, so a manual rehearsal never publishes by
  // accident. Only a tag with dry_run off publishes.
  process.stdout.write(`version=${version}\npublish=${dryRun ? "false" : "true"}\n`);
} else if (dryRun) {
  // A manual run without a tag builds and packs everything but publishes nothing.
  process.stdout.write(`version=${version}\npublish=false\n`);
} else {
  console.error("A run can only publish from a v* tag. Push the tag, or set dry_run.");
  process.exit(1);
}
