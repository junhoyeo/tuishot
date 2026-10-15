/** An RGB color with eight bits per channel. */
export interface Rgb {
  r: number;
  g: number;
  b: number;
}

/** A terminal color before it is resolved against a theme. Mirrors tuishot-core's Serde shape. */
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

/** The exact JSON shape serialized by tuishot-core::Grid. */
export interface Grid {
  columns: number;
  rows: number;
  cells: Cell[];
  cursor: Cursor | null;
}

export type OutputFormat = "png" | "svg" | "txt" | "json";
export type WindowChrome = "none" | "macos";
export type Duration = `${number}ms` | `${number}s` | `${number}m` | `${number}h`;
export type Size = `${number}x${number}`;
export type Scale = 1 | 2 | 3 | 4;

export interface ShootOptions {
  command: string;
  args?: readonly string[];
  /** Terminal dimensions in the native `COLUMNSxROWS` form. */
  size?: Size;
  waitText?: string;
  waitRegex?: string;
  /** Native duration string, for example `300ms` or `2s`. */
  waitStable?: Duration;
  keys?: readonly string[];
  env?: Readonly<Record<string, string>>;
  envClear?: boolean;
  /** The captured child working directory. It never changes the native process cwd. */
  cwd?: string;
  /** The captured child HOME. It never changes the native process environment. */
  home?: string;
  formats?: readonly OutputFormat[];
  scale?: Scale;
  chrome?: WindowChrome;
  /** Native duration string. JavaScript does not add a second timeout. */
  timeout?: Duration;
}

export interface ShootResult {
  png?: Buffer;
  svg?: string;
  txt?: string;
  json?: Grid;
}
