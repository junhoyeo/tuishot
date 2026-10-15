import { spawnSync } from "node:child_process";
import { resolveNativeBinary } from "./resolver.js";

/** Entrypoint used only by the package bin; importing the package root has no side effects. */
export function main(argv: readonly string[] = process.argv.slice(2)): number {
  let binary: string;
  try {
    binary = resolveNativeBinary();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    return 1;
  }
  const result = spawnSync(binary, argv, { stdio: "inherit" });
  if (result.error) {
    console.error(`tuishot failed to launch ${binary}: ${result.error.message}`);
    return 1;
  }
  if (result.signal) {
    console.error(`tuishot native process terminated by signal ${result.signal}`);
    return 1;
  }
  return result.status ?? 1;
}
