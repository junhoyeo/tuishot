import { createRuntime } from "./runtime.js";

const api = createRuntime(() => {
  if (!globalThis.Deno?.readFile) throw new Error("@tuishot/wasm/deno requires Deno.readFile");
  return globalThis.Deno.readFile(new URL("./dist/tuishot_wasm_bg.wasm", import.meta.url));
});
export const { init, Renderer, version, renderGrid, renderAnsi, gridToPng, gridToSvg, gridToText, gridToJson, ansiToGrid, ansiToPng, ansiToSvg, ansiToText, ansiToJson } = api;
export default api;
