import { createRuntime } from "./runtime.js";

const api = createRuntime(() => fetch(new URL("./dist/tuishot_wasm_bg.wasm", import.meta.url)));
export const { init, Renderer, version, renderGrid, renderAnsi, gridToPng, gridToSvg, gridToText, gridToJson, ansiToGrid, ansiToPng, ansiToSvg, ansiToText, ansiToJson } = api;
export default api;
