import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
export const targetsPath = resolve(repositoryRoot, "npm/targets.json");

const expectedPackages = [
  "@tuishot/cli-darwin-arm64",
  "@tuishot/cli-darwin-x64",
  "@tuishot/cli-linux-x64-gnu",
  "@tuishot/cli-linux-x64-musl",
  "@tuishot/cli-linux-arm64-gnu",
  "@tuishot/cli-linux-arm64-musl",
  "@tuishot/cli-win32-x64-msvc",
  "@tuishot/cli-win32-arm64-msvc",
];

const expectedTargets = [
  "aarch64-apple-darwin",
  "x86_64-apple-darwin",
  "x86_64-unknown-linux-gnu",
  "x86_64-unknown-linux-musl",
  "aarch64-unknown-linux-gnu",
  "aarch64-unknown-linux-musl",
  "x86_64-pc-windows-msvc",
  "aarch64-pc-windows-msvc",
];

export function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

export function canonicalJson(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

export function readCargoVersion() {
  const cargo = readFileSync(resolve(repositoryRoot, "Cargo.toml"), "utf8");
  const marker = "[workspace.package]";
  const markerIndex = cargo.indexOf(marker);
  const remainder = markerIndex === -1 ? "" : cargo.slice(markerIndex + marker.length);
  const nextSection = remainder.search(/\n\[/);
  const section = nextSection === -1 ? remainder : remainder.slice(0, nextSection);
  const version = section.match(/^version\s*=\s*"([^"]+)"\s*$/m)?.[1];
  if (!version) {
    throw new Error("Could not read [workspace.package].version from Cargo.toml");
  }
  return version;
}

export function readTargets() {
  const matrix = readJson(targetsPath);
  if (matrix.schemaVersion !== 1 || !Array.isArray(matrix.targets)) {
    throw new Error("npm/targets.json must use schemaVersion 1 and contain a targets array");
  }
  const packages = matrix.targets.map((target) => target.package);
  const rustTargets = matrix.targets.map((target) => target.target);
  if (JSON.stringify(packages) !== JSON.stringify(expectedPackages)) {
    throw new Error(
      `npm/targets.json must contain exactly these native packages in order:\n${expectedPackages.join("\n")}`,
    );
  }
  if (JSON.stringify(rustTargets) !== JSON.stringify(expectedTargets)) {
    throw new Error(
      `npm/targets.json must contain exactly these Rust targets in order:\n${expectedTargets.join("\n")}`,
    );
  }

  const identities = new Set();
  for (const target of matrix.targets) {
    const identity = `${target.platform}/${target.arch}/${target.libc ?? "none"}`;
    if (identities.has(identity)) {
      throw new Error(`Duplicate target identity ${identity}`);
    }
    identities.add(identity);
    if (!['darwin', 'linux', 'win32'].includes(target.platform)) {
      throw new Error(`Invalid Node platform ${target.platform} for ${target.target}`);
    }
    if (!['arm64', 'x64'].includes(target.arch)) {
      throw new Error(`Invalid Node architecture ${target.arch} for ${target.target}`);
    }
    if (target.platform === "linux") {
      if (!['gnu', 'musl'].includes(target.libc)) {
        throw new Error(`Linux target ${target.target} must declare libc gnu or musl`);
      }
      const expectedNpmLibc = target.libc === "gnu" ? "glibc" : "musl";
      if (target.npmLibc !== expectedNpmLibc) {
        throw new Error(`${target.target} must use npm libc ${expectedNpmLibc}`);
      }
    } else if (target.libc !== undefined || target.npmLibc !== undefined) {
      throw new Error(`Non-Linux target ${target.target} must not declare libc`);
    }
    const expectedArch = target.target.startsWith("aarch64-") ? "arm64" : target.target.startsWith("x86_64-") ? "x64" : undefined;
    const expectedPlatform = target.target.includes("-apple-darwin")
      ? "darwin"
      : target.target.includes("-unknown-linux-")
        ? "linux"
        : target.target.includes("-pc-windows-msvc")
          ? "win32"
          : undefined;
    const expectedLibc = target.target.endsWith("-linux-gnu")
      ? "gnu"
      : target.target.endsWith("-linux-musl")
        ? "musl"
        : undefined;
    if (target.arch !== expectedArch || target.platform !== expectedPlatform || target.libc !== expectedLibc) {
      throw new Error(`Node platform fields do not match Rust target ${target.target}`);
    }
    const expectedPackage = `@tuishot/cli-${target.platform}-${target.arch}${
      target.platform === "linux" ? `-${target.libc}` : target.platform === "win32" ? "-msvc" : ""
    }`;
    if (target.package !== expectedPackage) {
      throw new Error(`${target.target} must map to ${expectedPackage}`);
    }
    const expectedBinary = target.platform === "win32" ? "tuishot.exe" : "tuishot";
    if (target.binary !== expectedBinary) {
      throw new Error(`${target.target} must carry ${expectedBinary}`);
    }
  }
  return matrix.targets;
}

export function platformManifest(target, version) {
  const manifest = {
    name: target.package,
    version,
    description: `tuishot native binary for ${target.platform} ${target.arch}${target.libc ? ` ${target.libc}` : ""}`,
    license: "MIT",
    engines: { node: ">=18" },
    os: [target.platform],
    cpu: [target.arch],
    main: `./bin/${target.binary}`,
    exports: {
      ".": `./bin/${target.binary}`,
      "./bin": `./bin/${target.binary}`,
      "./package.json": "./package.json",
    },
    files: [`bin/${target.binary}`, "LICENSE-MIT"],
    repository: {
      type: "git",
      url: "git+https://github.com/junhoyeo/tuishot.git",
      directory: `npm/platforms/${target.target}`,
    },
    publishConfig: {
      access: "public",
      registry: "https://registry.npmjs.org/",
    },
  };
  if (target.npmLibc) {
    manifest.libc = [target.npmLibc];
  }
  return manifest;
}

export function generatedTargetsTypeScript(targets) {
  const runtimeTargets = targets.map(({ target, package: packageName, platform, arch, libc, binary }) => ({
    target,
    package: packageName,
    platform,
    arch,
    ...(libc ? { libc } : {}),
    binary,
  }));
  return `// Generated from npm/targets.json by npm/scripts/sync-packages.mjs. Do not edit.\n` +
    `export type LibcKind = "gnu" | "musl";\n\n` +
    `export interface NativeTarget {\n` +
    `  readonly target: string;\n` +
    `  readonly package: string;\n` +
    `  readonly platform: NodeJS.Platform;\n` +
    `  readonly arch: NodeJS.Architecture;\n` +
    `  readonly libc?: LibcKind;\n` +
    `  readonly binary: string;\n` +
    `}\n\n` +
    `export const TARGETS = ${JSON.stringify(runtimeTargets, null, 2)} as const satisfies readonly NativeTarget[];\n`;
}
