import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import test from "node:test";

import api, {
  Renderer,
  ansiToGrid,
  init,
  renderAnsi,
  renderGrid,
  version,
} from "../node.js";

const PNG_MAGIC = [137, 80, 78, 71, 13, 10, 26, 10];
const ANSI = [
  "\x1b[1;4:3;38;5;196;48;2;1;2;3;58:2::4:5:6mA\x1b[0m界",
  "\n",
  "e\u{301}\x1b[2CB\x1b[1D\x1b[KZ",
].join("");
const EXPECTED_PNG_SHA256 = "92e73f5a0ef6d1ae2e533d563fadf8c59acff3e222952dfd1f704d1e620ac5c3";

function pngDimensions(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  return [view.getUint32(16), view.getUint32(20)];
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

test("runtime entry modules construct without loading wasm", async () => {
  for (const entry of ["../browser.js", "../bun.js", "../deno.js"]) {
    const module = await import(entry);
    assert.equal(typeof module.init, "function");
    assert.equal(typeof module.Renderer, "function");
    assert.equal(typeof module.renderAnsi, "function");
  }
});

test("Node fs initialization is idempotent and exposes the pinned crate version", async () => {
  assert.throws(() => new Renderer(), /not initialized/);
  await Promise.all([init(), init(), api.init()]);
  assert.equal(version(), "0.1.0");
});

test("ANSI renders deterministically to PNG, SVG, text, and canonical Grid", async () => {
  const first = await renderAnsi(ANSI, { cols: 10, rows: 3 });
  const second = await renderAnsi(ANSI, { cols: 10, rows: 3 });
  assert.ok(first.png instanceof Uint8Array);
  assert.deepEqual([...first.png.subarray(0, 8)], PNG_MAGIC);
  assert.deepEqual(pngDimensions(first.png), [132, 92]);
  assert.equal(sha256(first.png), EXPECTED_PNG_SHA256);
  assert.deepEqual(second.png, first.png);
  assert.equal(second.svg, first.svg);
  assert.equal(second.txt, first.txt);
  assert.deepEqual(second.json, first.json);
  assert.match(first.svg, /^<svg xmlns=/);
  assert.equal(first.txt.split("\n").length, 3);
  assert.equal(first.json.columns, 10);
  assert.equal(first.json.rows, 3);
  assert.equal(first.json.cells.length, 30);

  const [styled, wide, continuation] = first.json.cells;
  assert.equal(styled.symbol, "A");
  assert.deepEqual(styled.style.foreground, { kind: "indexed", value: 196 });
  assert.deepEqual(styled.style.background, { kind: "rgb", value: { r: 1, g: 2, b: 3 } });
  assert.deepEqual(styled.style.underline_color, { kind: "rgb", value: { r: 4, g: 5, b: 6 } });
  assert.equal(styled.style.attributes.bold, true);
  assert.equal(styled.style.attributes.underline, "curly");
  assert.deepEqual({ symbol: wide.symbol, width: wide.width }, { symbol: "界", width: "wide" });
  assert.deepEqual(
    { symbol: continuation.symbol, width: continuation.width },
    { symbol: "", width: "continuation" },
  );
});

test("synchronous Renderer and async Grid helpers agree exactly", async () => {
  const grid = await ansiToGrid(ANSI, { cols: 10, rows: 3 });
  const oneShot = await renderGrid(grid, { theme: "light", scale: 2, chrome: "macos" });
  const renderer = new Renderer({ theme: "light", scale: 2, chrome: "macos" });
  try {
    const direct = renderer.renderGrid(grid);
    assert.deepEqual(direct.png, oneShot.png);
    assert.equal(direct.svg, oneShot.svg);
    assert.equal(direct.txt, oneShot.txt);
    assert.deepEqual(direct.json, oneShot.json);
    assert.equal(renderer.gridToJson(grid), JSON.stringify(direct.json));
    assert.deepEqual(renderer.ansiToGrid(ANSI, { cols: 10, rows: 3 }), grid);
  } finally {
    renderer.free();
  }
});

test("malformed options and grids fail through Rust core validation", async () => {
  assert.throws(() => new Renderer({ scale: 5 }), /scale must be between 1 and 4/);
  assert.throws(() => new Renderer({ theme: "sepia" }), /invalid theme.*sepia/);
  assert.throws(() => new Renderer({ filesystemFont: "font.ttf" }), /unknown field.*filesystemFont/);
  await assert.rejects(renderAnsi("x", { cols: 0, rows: 1 }), /columns must be between 1 and 65535/);
  await assert.rejects(renderAnsi("x", { cols: Number.NaN, rows: 1 }), /columns must be finite/);
  await assert.rejects(renderAnsi("x", { cols: 1.5, rows: 1 }), /columns must be an integer/);
  await assert.rejects(renderAnsi("x", { cols: 1_001, rows: 1_000 }), /must not exceed 1000000 cells/);

  const grid = await ansiToGrid("A界", { cols: 4, rows: 2 });
  await assert.rejects(renderGrid({ ...grid, columns: 0 }), /terminal grid dimensions must be non-zero/);
  await assert.rejects(renderGrid({ ...grid, cells: grid.cells.slice(1) }), /grid dimensions require 8 cells/);
  const brokenWide = structuredClone(grid);
  brokenWide.cells[2] = { ...brokenWide.cells[2], width: "single", symbol: " " };
  await assert.rejects(renderGrid(brokenWide), /wide-cell invariant is broken/);
});

test("browser entry accepts explicit bytes without filesystem APIs", async () => {
  const browser = await import("../browser.js");
  const bytes = await readFile(new URL("../dist/tuishot_wasm_bg.wasm", import.meta.url));
  await browser.init(bytes);
  const result = await browser.renderAnsi("browser", { cols: 8, rows: 1 });
  assert.deepEqual([...result.png.subarray(0, 8)], PNG_MAGIC);
});
