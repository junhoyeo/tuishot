import { describe, expect, it } from "vitest";
import { renderAnsi } from "../../../npm/node/index.js";
import { renderExampleToAnsi } from "../src/example.jsx";

describe("Ink renderToString", () => {
  it("matches its ANSI and tuishot text snapshots", () => {
    const ansi = renderExampleToAnsi();
    const rendered = renderAnsi(ansi, {
      cols: 24,
      rows: 4,
      formats: ["txt", "json"],
    });

    expect(ansi).toMatchSnapshot("ansi");
    expect(rendered.txt).toMatchSnapshot("tuishot text");
    expect(rendered.json.cells).toHaveLength(96);
    const styled = rendered.json.cells[24 + 2];
    expect(styled.style.attributes.bold).toBe(true);
    expect(styled.style.foreground).toEqual({ kind: "indexed", value: 2 });
  });
});
