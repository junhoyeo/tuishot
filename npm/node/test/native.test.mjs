import assert from "node:assert/strict";
import { createRequire } from "node:module";
import test from "node:test";

import esmBindings, { renderAnsi, renderGrid } from "../index.js";

const require = createRequire(import.meta.url);
const cjsBindings = require("../index.cjs");
const PNG_MAGIC = [137, 80, 78, 71, 13, 10, 26, 10];
const ANSI = [
  "\x1b[1;4;38;5;196;48;2;1;2;3;58;2;4;5;6mA\x1b[0m界",
  "\n",
  "\x1b[2CB\x1b[1D\x1b[KZ",
].join("");

function assertAllOutputs(result) {
  assert.ok(Buffer.isBuffer(result.png));
  assert.deepEqual([...result.png.subarray(0, 8)], PNG_MAGIC);
  assert.match(result.svg, /^<svg xmlns=/);
  assert.equal(typeof result.txt, "string");
  assert.equal(result.json.columns, 10);
  assert.equal(result.json.rows, 3);
  assert.equal(result.json.cells.length, 30);
  assert.ok(result.json.cursor === null || Number.isInteger(result.json.cursor.x));
}

test("ESM and CommonJS expose the same in-process functions", () => {
  assert.equal(typeof renderAnsi, "function");
  assert.equal(typeof renderGrid, "function");
  assert.equal(esmBindings.renderAnsi, renderAnsi);
  assert.equal(cjsBindings.renderAnsi, renderAnsi);
  assert.equal(cjsBindings.renderGrid, renderGrid);
});

test("renderAnsi returns all four formats by default with exact grid invariants", () => {
  const result = renderAnsi(ANSI, { cols: 10, rows: 3 });
  assertAllOutputs(result);

  const [styled, wide, continuation] = result.json.cells;
  assert.equal(styled.symbol, "A");
  assert.deepEqual(styled.style.foreground, { kind: "indexed", value: 196 });
  assert.deepEqual(styled.style.background, { kind: "rgb", value: { r: 1, g: 2, b: 3 } });
  assert.deepEqual(styled.style.underline_color, { kind: "rgb", value: { r: 4, g: 5, b: 6 } });
  assert.equal(styled.style.attributes.bold, true);
  assert.equal(styled.style.attributes.underline, "single");
  assert.deepEqual({ symbol: wide.symbol, width: wide.width }, { symbol: "界", width: "wide" });
  assert.deepEqual({ symbol: continuation.symbol, width: continuation.width }, { symbol: "", width: "continuation" });
  assert.equal(result.txt.split("\n")[1].startsWith("  Z"), true, "bare LF must reset to column zero");
});

test("renderGrid validates the Rust wire model and renders every output", () => {
  const parsed = renderAnsi(ANSI, { cols: 10, rows: 3 });
  const rendered = renderGrid(parsed.json);
  assertAllOutputs(rendered);
  assert.deepEqual(rendered.png, parsed.png);
  assert.equal(rendered.svg, parsed.svg);
  assert.equal(rendered.txt, parsed.txt);
  assert.deepEqual(rendered.json, parsed.json);
});

test("repeated deterministic renders are byte-identical", () => {
  const options = {
    cols: 10,
    rows: 3,
    scale: 2,
    theme: "light",
    chrome: "macos",
    formats: ["png", "svg", "txt", "json"],
  };
  const first = renderAnsi(ANSI, options);
  const second = renderAnsi(ANSI, options);
  assert.deepEqual(second.png, first.png);
  assert.equal(second.svg, first.svg);
  assert.equal(second.txt, first.txt);
  assert.deepEqual(second.json, first.json);
});

test("all documented scales, themes, and chrome modes render", () => {
  for (const scale of [1, 2, 3, 4]) {
    for (const theme of ["dark", "light"]) {
      for (const chrome of ["none", "macos"]) {
        const result = renderAnsi("matrix", {
          cols: 8,
          rows: 2,
          scale,
          theme,
          chrome,
          formats: ["png"],
        });
        assert.deepEqual([...result.png.subarray(0, 8)], PNG_MAGIC);
      }
    }
  }
});

test("format selection is exact and rejects empty or unknown sets", () => {
  const result = renderAnsi("only text", { cols: 12, rows: 1, formats: ["txt", "txt"] });
  assert.match(result.txt, /^only text/);
  assert.equal(result.png, undefined);
  assert.equal(result.svg, undefined);
  assert.equal(result.json, undefined);
  assert.throws(() => renderAnsi("x", { cols: 1, rows: 1, formats: [] }), /formats must not be empty/);
  assert.throws(() => renderAnsi("x", { cols: 1, rows: 1, formats: ["pdf"] }), /invalid format.*pdf/);
});

test("numbers and enums are rejected before conversion", () => {
  for (const cols of [NaN, Infinity, -Infinity]) {
    assert.throws(() => renderAnsi("x", { cols, rows: 1 }), /cols must be finite/);
  }
  assert.throws(() => renderAnsi("x", { cols: 1.5, rows: 1 }), /cols must be an integer/);
  assert.throws(() => renderAnsi("x", { cols: 0, rows: 1 }), /cols must be between 1 and 65535/);
  assert.throws(() => renderAnsi("x", { cols: 65_536, rows: 1 }), /cols must be between 1 and 65535/);
  assert.throws(() => renderAnsi("x", { cols: 65_535, rows: 65_535 }), /must not exceed 1000000 cells/);
  assert.throws(() => renderAnsi("x", { cols: 1, rows: 1.5 }), /rows must be an integer/);
  assert.throws(() => renderAnsi("x", { cols: 1, rows: 1, scale: NaN }), /scale must be finite/);
  assert.throws(() => renderAnsi("x", { cols: 1, rows: 1, scale: 2.5 }), /scale must be an integer/);
  assert.throws(() => renderAnsi("x", { cols: 1, rows: 1, scale: 5 }), /scale must be between 1 and 4/);
  assert.throws(() => renderAnsi("x", { cols: 1, rows: 1, theme: "sepia" }), /invalid theme.*sepia/);
  assert.throws(() => renderAnsi("x", { cols: 1, rows: 1, chrome: "windows" }), /invalid chrome.*windows/);
});

test("invalid or excessive JavaScript grids are rejected before rendering", () => {
  const valid = renderAnsi("A界", { cols: 4, rows: 2, formats: ["json"] }).json;

  assert.throws(
    () => renderGrid({ ...valid, columns: 1_001, rows: 1_000 }),
    /grid columns × rows must not exceed 1000000 cells/,
  );
  const oversizedSymbol = structuredClone(renderAnsi("x", { cols: 1, rows: 1, formats: ["json"] }).json);
  oversizedSymbol.cells[0].symbol = "x".repeat(16 * 1024 * 1024 + 1);
  assert.throws(() => renderGrid(oversizedSymbol), /grid symbols must not exceed 16777216 UTF-8 bytes/);

  assert.throws(() => renderGrid({ ...valid, columns: 0 }), /terminal grid dimensions must be non-zero/);
  assert.throws(() => renderGrid({ ...valid, cells: valid.cells.slice(1) }), /grid dimensions require 8 cells/);
  assert.throws(
    () => renderGrid({ ...valid, cursor: { x: 4, y: 0, shape: "block", visible: true } }),
    /cursor \(4, 0\) is outside the grid/,
  );

  const brokenWide = structuredClone(valid);
  brokenWide.cells[2] = { ...brokenWide.cells[2], width: "single", symbol: " " };
  assert.throws(() => renderGrid(brokenWide), /wide-cell invariant is broken/);

  const badUnderlineColor = structuredClone(valid);
  badUnderlineColor.cells[0].style.underline_color = { kind: "indexed", value: 256 };
  assert.throws(() => renderGrid(badUnderlineColor), /invalid value: integer `256`/);
});
