// Generated from npm/targets.json by npm/scripts/sync-packages.mjs. Do not edit.
export type LibcKind = "gnu" | "musl";

export interface NativeTarget {
  readonly target: string;
  readonly package: string;
  readonly platform: NodeJS.Platform;
  readonly arch: NodeJS.Architecture;
  readonly libc?: LibcKind;
  readonly binary: string;
}

export const TARGETS = [
  {
    "target": "aarch64-apple-darwin",
    "package": "@tuishot/cli-darwin-arm64",
    "platform": "darwin",
    "arch": "arm64",
    "binary": "tuishot"
  },
  {
    "target": "x86_64-apple-darwin",
    "package": "@tuishot/cli-darwin-x64",
    "platform": "darwin",
    "arch": "x64",
    "binary": "tuishot"
  },
  {
    "target": "x86_64-unknown-linux-gnu",
    "package": "@tuishot/cli-linux-x64-gnu",
    "platform": "linux",
    "arch": "x64",
    "libc": "gnu",
    "binary": "tuishot"
  },
  {
    "target": "x86_64-unknown-linux-musl",
    "package": "@tuishot/cli-linux-x64-musl",
    "platform": "linux",
    "arch": "x64",
    "libc": "musl",
    "binary": "tuishot"
  },
  {
    "target": "aarch64-unknown-linux-gnu",
    "package": "@tuishot/cli-linux-arm64-gnu",
    "platform": "linux",
    "arch": "arm64",
    "libc": "gnu",
    "binary": "tuishot"
  },
  {
    "target": "aarch64-unknown-linux-musl",
    "package": "@tuishot/cli-linux-arm64-musl",
    "platform": "linux",
    "arch": "arm64",
    "libc": "musl",
    "binary": "tuishot"
  }
] as const satisfies readonly NativeTarget[];
