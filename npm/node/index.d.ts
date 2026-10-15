/** An RGB color with eight bits per channel. */
export interface Rgb {
  r: number;
  g: number;
  b: number;
}

/** Exact tagged-color shape used by tuishot-core's Serde wire format. */
export type Color =
  | { kind: "default" }
  | { kind: "indexed"; value: number }
  | { kind: "rgb"; value: Rgb };

export type Underline = "none" | "single" | "double" | "curly" | "dotted" | "dashed";
export type Blink = "none" | "slow" | "rapid";
export type Frame = "none" | "framed" | "encircled";
export type Ideogram =
  | "none"
  | "underline"
  | "double_underline"
  | "overline"
  | "double_overline"
  | "stress";

export interface Attributes {
  bold: boolean;
  dim: boolean;
  italic: boolean;
  underline: Underline;
  blink: Blink;
  reverse: boolean;
  hidden: boolean;
  strikethrough: boolean;
  frame: Frame;
  overline: boolean;
  ideogram: Ideogram;
}

export interface Style {
  foreground: Color;
  background: Color;
  /** SGR 58 underline color. */
  underline_color: Color;
  attributes: Attributes;
}

export type CellWidth = "single" | "wide" | "continuation";

export interface Cell {
  symbol: string;
  width: CellWidth;
  style: Style;
}

export type CursorShape = "block" | "bar" | "underline";

export interface Cursor {
  x: number;
  y: number;
  shape: CursorShape;
  visible: boolean;
}

/** The exact JSON shape serialized and validated by tuishot-core::Grid. */
export interface Grid {
  columns: number;
  rows: number;
  cells: Cell[];
  cursor: Cursor | null;
}

export type OutputFormat = "png" | "svg" | "txt" | "json";
export type Scale = 1 | 2 | 3 | 4;
export type Theme = "dark" | "light";
export type WindowChrome = "none" | "macos";

export interface RenderOptions {
  /** Integer output scale. Defaults to 1. */
  scale?: Scale;
  /** Built-in deterministic palette. Defaults to dark. */
  theme?: Theme;
  /** Window decoration. Defaults to none. */
  chrome?: WindowChrome;
  /** Requested outputs. Defaults to all four formats. Must not be empty. */
  formats?: readonly OutputFormat[];
}

export interface RenderAnsiOptions extends RenderOptions {
  /** Terminal columns. Must be a finite integer from 1 through 65535; combined area is at most 1,000,000 cells. */
  cols: number;
  /** Terminal rows. Must be a finite integer from 1 through 65535. */
  rows: number;
}

export interface RenderResult {
  png?: Buffer;
  svg?: string;
  txt?: string;
  json?: Grid;
}

/**
 * Parse ANSI without a process or PTY and render the resulting grid.
 * Bare LF starts a new row at column zero; existing CRLF is preserved.
 */
export function renderAnsi(ansi: string, options: RenderAnsiOptions): RenderResult;

/**
 * Validate a Grid through tuishot-core's custom Serde deserializer and render it.
 * Inputs are limited to 1,000,000 cells and 16 MiB of aggregate symbol text.
 */
export function renderGrid(grid: Grid, options?: RenderOptions): RenderResult;

declare const bindings: {
  renderAnsi: typeof renderAnsi;
  renderGrid: typeof renderGrid;
};
export default bindings;
