import { init, renderAnsi } from "../deno.js";
await init();
const result = await renderAnsi("Deno 界", { cols: 8, rows: 2 });
if (!(result.png instanceof Uint8Array)) throw new Error("PNG is not Uint8Array");
if (result.png[0] !== 137 || result.png[1] !== 80) throw new Error("PNG signature mismatch");
console.log("Deno loader smoke passed.");
