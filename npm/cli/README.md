# @tuishot/cli

Native tuishot CLI and typed Node.js API for Node 18 and newer.

```sh
npm install --save-dev @tuishot/cli
tuishot --size 120x40 --wait-text Ready -o dashboard.png -- my-tui
```

```ts
import { writeFile } from "node:fs/promises";
import { shoot, runScenes } from "@tuishot/cli";

const capture = await shoot({
  command: "my-tui",
  args: ["--fixture", "demo.json"],
  size: "120x40",
  waitText: "Ready",
  formats: ["png", "json"],
});

await writeFile("dashboard.png", capture.png!);
console.log(capture.json?.cells.length);

await runScenes("tuishot.toml");
```

`shoot` invokes one platform-specific native binary and returns every requested format from one captured grid. The binary is installed through an exact-version optional package. The launcher never searches `PATH` for a fallback `tuishot` executable.

The unscoped [`tuishot`](https://www.npmjs.com/package/tuishot) package is an alias that re-exports this API.
