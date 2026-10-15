import { execFileSync } from "node:child_process";
import { existsSync, readdirSync } from "node:fs";
import { createRequire } from "node:module";
import { TARGETS, type LibcKind, type NativeTarget } from "./targets.generated.js";

export interface LibcDetectionDependencies {
  readonly report?: unknown;
  readonly runLdd?: () => string;
  readonly listDirectory?: (path: string) => readonly string[];
  readonly fileExists?: (path: string) => boolean;
}

function reportLibc(report: unknown): LibcKind | undefined {
  if (!report || typeof report !== "object") return undefined;
  const value = report as {
    header?: { glibcVersionRuntime?: unknown; release?: { sourceUrl?: unknown } };
    sharedObjects?: unknown;
  };
  if (typeof value.header?.glibcVersionRuntime === "string" && value.header.glibcVersionRuntime) {
    return "gnu";
  }
  const shared = value.sharedObjects;
  if (
    Array.isArray(shared) &&
    shared.some((entry) => typeof entry === "string" && /(?:^|[/\\-])(?:libc\.musl|ld-musl|musl)/i.test(entry))
  ) {
    return "musl";
  }
  const sourceUrl = value.header?.release?.sourceUrl;
  if (typeof sourceUrl === "string" && sourceUrl.toLowerCase().includes("musl")) {
    return "musl";
  }
  return undefined;
}

function textLibc(text: string): LibcKind | undefined {
  const lower = text.toLowerCase();
  if (lower.includes("musl")) return "musl";
  if (lower.includes("glibc") || lower.includes("gnu libc") || lower.includes("gnu c library")) {
    return "gnu";
  }
  return undefined;
}

/** Detect Linux's C library without an override or an unsafe GNU default. */
export function detectLibc(dependencies: LibcDetectionDependencies = {}): LibcKind {
  const report = dependencies.report ?? process.report?.getReport?.();
  const fromReport = reportLibc(report);
  if (fromReport) return fromReport;

  const runLdd = dependencies.runLdd ?? (() => {
    try {
      return execFileSync("ldd", ["--version"], {
        encoding: "utf8",
        stdio: ["ignore", "pipe", "pipe"],
      });
    } catch (error) {
      const failure = error as { stdout?: unknown; stderr?: unknown };
      const stringify = (value: unknown): string =>
        Buffer.isBuffer(value) ? value.toString("utf8") : typeof value === "string" ? value : "";
      return `${stringify(failure.stdout)}\n${stringify(failure.stderr)}`;
    }
  });
  const fromLdd = textLibc(runLdd());
  if (fromLdd) return fromLdd;

  const listDirectory = dependencies.listDirectory ?? ((path: string) => readdirSync(path));
  const fileExists = dependencies.fileExists ?? existsSync;
  const loaders: string[] = [];
  for (const directory of ["/lib", "/lib64", "/usr/lib", "/usr/lib64"]) {
    try {
      for (const entry of listDirectory(directory)) {
        if (/^(?:ld-musl-|ld-linux-|ld64\.so|ld-linux\.so)/.test(entry)) {
          loaders.push(entry.toLowerCase());
        }
      }
    } catch {
      // Missing and unreadable loader directories are inconclusive.
    }
  }
  const hasMusl = loaders.some((entry) => entry.startsWith("ld-musl-"));
  const hasGnu = loaders.some((entry) => /^(?:ld-linux-|ld64\.so|ld-linux\.so)/.test(entry));
  if (hasMusl && !hasGnu) return "musl";
  if (hasGnu && !hasMusl) return "gnu";
  if (hasMusl && hasGnu && fileExists("/etc/alpine-release")) return "musl";

  throw new Error(
    "tuishot could not determine the Linux C library (glibc or musl) from process.report, ldd, or dynamic loaders; refusing to guess a native package",
  );
}

export interface TargetRequest {
  readonly platform?: NodeJS.Platform;
  readonly arch?: NodeJS.Architecture;
  readonly libc?: LibcKind;
  readonly detectLibc?: () => LibcKind;
}

export function resolveTarget(request: TargetRequest = {}): NativeTarget {
  const platform = request.platform ?? process.platform;
  const arch = request.arch ?? process.arch;
  let libc = request.libc;
  if (platform === "linux" && libc === undefined) {
    libc = (request.detectLibc ?? detectLibc)();
  }
  const target = TARGETS.find(
    (candidate) =>
      candidate.platform === platform &&
      candidate.arch === arch &&
      (candidate.platform !== "linux" || ("libc" in candidate && candidate.libc === libc)),
  );
  if (!target) {
    const suffix = platform === "linux" ? `/${libc ?? "unknown-libc"}` : "";
    throw new Error(
      `tuishot does not provide a native binary for ${platform}/${arch}${suffix}; supported targets: ${TARGETS.map((candidate) => `${candidate.platform}/${candidate.arch}${"libc" in candidate ? `/${candidate.libc}` : ""}`).join(", ")}`,
    );
  }
  return target;
}

export function resolveNativeBinary(request: TargetRequest = {}): string {
  const target = resolveTarget(request);
  const require = createRequire(__filename);
  try {
    return require.resolve(`${target.package}/bin`);
  } catch (cause) {
    const error = cause as NodeJS.ErrnoException;
    throw new Error(
      `tuishot native package ${target.package} is not installed for ${target.platform}/${target.arch}${target.libc ? `/${target.libc}` : ""}. Reinstall @tuishot/cli without omitting optional dependencies.`,
      { cause: error },
    );
  }
}
