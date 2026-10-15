import { spawn } from "node:child_process";
import { resolveNativeBinary } from "./resolver.js";

export interface NativeResult {
  readonly stdout: Buffer;
  readonly stderr: Buffer;
}

export class NativeCommandError extends Error {
  readonly command: string;
  readonly args: readonly string[];
  readonly stderr: string;
  readonly code: number | null;
  readonly signal: NodeJS.Signals | null;

  constructor(
    command: string,
    args: readonly string[],
    stderr: string,
    code: number | null,
    signal: NodeJS.Signals | null,
    cause?: unknown,
  ) {
    const outcome = code === null ? `signal ${signal ?? "unknown"}` : `code ${code}`;
    const details = stderr ? `\nstderr:\n${stderr}` : "\nstderr: <empty>";
    super(`tuishot native command failed (${outcome}): ${command} ${args.join(" ")}${details}`, {
      cause,
    });
    this.name = "NativeCommandError";
    this.command = command;
    this.args = args;
    this.stderr = stderr;
    this.code = code;
    this.signal = signal;
  }
}

export interface RunNativeOptions {
  /** Internal test seam. Public APIs intentionally use the current cwd and environment. */
  readonly binary?: string;
}

export function runNative(args: readonly string[], options: RunNativeOptions = {}): Promise<NativeResult> {
  const binary = options.binary ?? resolveNativeBinary();
  return new Promise((resolve, reject) => {
    const child = spawn(binary, args, {
      stdio: "pipe",
      // Deliberately do not set cwd or env. ShootOptions.cwd/home/env belong to
      // the child captured by native tuishot, never to this wrapper process.
    });
    const stdout: Buffer[] = [];
    const stderr: Buffer[] = [];
    child.stdout.on("data", (chunk: Buffer | string) => stdout.push(Buffer.from(chunk)));
    child.stderr.on("data", (chunk: Buffer | string) => stderr.push(Buffer.from(chunk)));
    child.once("error", (cause) => {
      reject(new NativeCommandError(binary, args, Buffer.concat(stderr).toString("utf8").trimEnd(), null, null, cause));
    });
    child.once("close", (code, signal) => {
      const stdoutBuffer = Buffer.concat(stdout);
      const stderrBuffer = Buffer.concat(stderr);
      if (code === 0) {
        resolve({ stdout: stdoutBuffer, stderr: stderrBuffer });
      } else {
        reject(
          new NativeCommandError(
            binary,
            args,
            stderrBuffer.toString("utf8").trimEnd(),
            code,
            signal,
          ),
        );
      }
    });
  });
}
