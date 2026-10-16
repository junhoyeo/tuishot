import assert from "node:assert/strict";
import { init, renderAnsi } from "../bun.js";
await init();
const result = await renderAnsi("Bun 界", { cols: 8, rows: 2 });
assert.ok(result.png instanceof Uint8Array);
assert.deepEqual([...result.png.slice(0, 8)], [137, 80, 78, 71, 13, 10, 26, 10]);
console.log("Bun loader smoke passed.");
