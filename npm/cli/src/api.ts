import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { isAbsolute, resolve } from "node:path";
import { runNative } from "./native.js";
import type { Grid, OutputFormat, ShootOptions, ShootResult } from "./types.js";

const OUTPUT_EXTENSIONS: Readonly<Record<OutputFormat, string>> = {
  png: "png",
  svg: "svg",
  txt: "txt",
  json: "json",
};
const VALID_FORMATS = new Set<OutputFormat>(["png", "svg", "txt", "json"]);

function pushValue(args: string[], flag: string, value: string | number | undefined): void {
  if (value !== undefined) args.push(`--${flag}=${value}`);
}

function validateString(value: string, label: string): void {
  if (value.length === 0) throw new TypeError(`${label} must not be empty`);
  if (value.includes("\0")) throw new TypeError(`${label} must not contain NUL`);
}

export function buildShootArgv(options: ShootOptions, outputBase: string): {
  readonly argv: string[];
  readonly formats: readonly OutputFormat[];
} {
  validateString(options.command, "command");
  validateString(outputBase, "output basename");
  const formats = options.formats === undefined ? (["png"] as const) : [...options.formats];
  if (formats.length === 0) throw new TypeError("formats must not be empty");
  const uniqueFormats: OutputFormat[] = [];
  for (const format of formats) {
    if (!VALID_FORMATS.has(format)) {
      throw new TypeError(`invalid format ${JSON.stringify(format)}; expected png, svg, txt, or json`);
    }
    if (!uniqueFormats.includes(format)) uniqueFormats.push(format);
  }

  const argv = [`--output=${outputBase}`, `--format=${uniqueFormats.join(",")}`];
  pushValue(argv, "size", options.size);
  pushValue(argv, "wait-text", options.waitText);
  pushValue(argv, "wait-regex", options.waitRegex);
  pushValue(argv, "wait-stable", options.waitStable);
  if (options.keys !== undefined) {
    for (const key of options.keys) validateString(key, "key");
    if (options.keys.length > 0) pushValue(argv, "keys", options.keys.join(","));
  }
  if (options.envClear) argv.push("--env-clear");
  if (options.env !== undefined) {
    for (const key of Object.keys(options.env).sort()) {
      const value = options.env[key];
      if (value === undefined) continue;
      if (!key || key.includes("=") || key.includes("\0")) {
        throw new TypeError(`invalid environment key ${JSON.stringify(key)}`);
      }
      if (value.includes("\0")) throw new TypeError(`environment value for ${JSON.stringify(key)} must not contain NUL`);
      argv.push(`--env=${key}=${value}`);
    }
  }
  pushValue(argv, "cwd", options.cwd);
  pushValue(argv, "home", options.home);
  pushValue(argv, "scale", options.scale);
  pushValue(argv, "chrome", options.chrome);
  pushValue(argv, "timeout", options.timeout);
  argv.push("--", options.command);
  for (const argument of options.args ?? []) {
    if (argument.includes("\0")) throw new TypeError("arguments must not contain NUL");
    argv.push(argument);
  }
  return { argv, formats: uniqueFormats };
}

/** Capture one terminal grid through exactly one native tuishot invocation. */
export async function shoot(options: ShootOptions): Promise<ShootResult> {
  const directory = await mkdtemp(`${tmpdir()}/tuishot-node-`);
  const outputBase = resolve(directory, "capture");
  try {
    const { argv, formats } = buildShootArgv(options, outputBase);
    await runNative(argv);
    const result: ShootResult = {};
    await Promise.all(
      formats.map(async (format) => {
        const contents = await readFile(`${outputBase}.${OUTPUT_EXTENSIONS[format]}`);
        switch (format) {
          case "png":
            result.png = contents;
            break;
          case "svg":
            result.svg = contents.toString("utf8");
            break;
          case "txt":
            result.txt = contents.toString("utf8");
            break;
          case "json":
            result.json = JSON.parse(contents.toString("utf8")) as Grid;
            break;
        }
      }),
    );
    return result;
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

/** Run a native scene file. Relative input is converted to an absolute path. */
export async function runScenes(path: string): Promise<void> {
  validateString(path, "scene path");
  const absolute = isAbsolute(path) ? path : resolve(path);
  await runNative(["run", absolute]);
}
