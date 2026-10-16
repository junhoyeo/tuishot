import { existsSync } from "node:fs";
import { win32 } from "node:path";

/** Execute npm's JavaScript entry point on Windows, never a .cmd via a shell. */
export function npmInvocation(args, {
  platform = process.platform,
  executable = process.execPath,
  env = process.env,
  exists = existsSync,
} = {}) {
  if (platform !== "win32") return { command: "npm", args };
  const candidates = [
    env.npm_execpath,
    win32.join(win32.dirname(executable), "node_modules", "npm", "bin", "npm-cli.js"),
  ];
  const cli = candidates.find((file) => file && /(?:^|[\\/])npm-cli\.js$/i.test(file) && exists(file));
  if (!cli) throw new Error("Cannot find npm-cli.js beside node.exe or in npm_execpath; install Node.js with npm");
  return { command: executable, args: [cli, ...args] };
}
