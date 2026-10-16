import { createRuntime } from "./runtime.js";

const api = createRuntime(() => {
  if (!globalThis.Bun?.file) throw new Error("@tuishot/wasm/bun requires Bun.file");
  return globalThis.Bun.file(new URL("./dist/tuishot_wasm_bg.wasm", import.meta.url)).arrayBuffer();
});
export const { init, Renderer, version, renderGrid, renderAnsi, gridToPng, gridToSvg, gridToText, gridToJson, ansiToGrid, ansiToPng, ansiToSvg, ansiToText, ansiToJson } = api;
export default api;
