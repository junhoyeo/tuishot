import { spawnSync } from "node:child_process";
import { repositoryRoot } from "./package-utils.mjs";

const result = spawnSync(
  "cargo",
  ["tree", "--locked", "--target", "wasm32-unknown-unknown", "-p", "tuishot-wasm", "--edges", "normal", "--prefix", "none"],
  { cwd: repositoryRoot, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 },
);
if (result.error) throw result.error;
if (result.status !== 0) throw new Error(result.stderr || result.stdout);
const packages = result.stdout
  .split(/\r?\n/)
  .map((line) => line.trim().match(/^([^ ]+) v[^ ]+/)?.[1])
  .filter(Boolean);
const forbidden = ["alacritty_terminal", "nix", "rustix", "rustix-openpty", "polling"];
const found = [...new Set(packages.filter((name) => forbidden.includes(name)))];
if (found.length) {
  throw new Error(`wasm dependency graph contains native-only crates: ${found.join(", ")}`);
}
for (const required of ["tuishot-ansi", "tuishot-core", "tuishot-render", "vte", "wasm-bindgen"]) {
  if (!packages.includes(required)) throw new Error(`wasm dependency graph is missing ${required}`);
}
console.log(`Checked ${new Set(packages).size} wasm dependencies; no native terminal/OS crates are present.`);
