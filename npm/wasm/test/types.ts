import { Renderer, init, renderAnsi, renderGrid, type Grid, type RenderOptions } from "../index.js";

declare const grid: Grid;
const options: RenderOptions = { scale: 2, theme: "dark", chrome: "macos" };
await init();
const renderer = new Renderer(options);
const png: Uint8Array = renderer.gridToPng(grid);
const svg: string = renderer.gridToSvg(grid);
const result = await renderAnsi("\x1b[31mtyped", { ...options, cols: 80, rows: 24 });
const again: Grid = (await renderGrid(result.json)).json;
void [png, svg, again];
renderer.free();
