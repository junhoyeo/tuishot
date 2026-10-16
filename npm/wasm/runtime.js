import initWasm, { NativeRenderer, version as nativeVersion } from "./dist/tuishot_wasm.js";

const notInitialized =
  "@tuishot/wasm is not initialized; call and await init() before constructing Renderer";

function dimensions(options) {
  if (options === null || typeof options !== "object") {
    throw new TypeError("ANSI options must be an object with cols and rows");
  }
  return [options.cols, options.rows];
}

function rendererOptions(options = {}) {
  const { cols: _cols, rows: _rows, ...renderer } = options;
  return renderer;
}

/** Create one runtime-specific public API around the single generated glue module. */
export function createRuntime(loadDefaultWasm) {
  let initialized = false;
  let initializing;

  async function init(input) {
    if (initialized) return;
    if (!initializing) {
      initializing = (async () => {
        const moduleOrPath = input === undefined ? await loadDefaultWasm() : await input;
        await initWasm({ module_or_path: moduleOrPath });
        initialized = true;
      })().catch((error) => {
        initializing = undefined;
        throw error;
      });
    }
    await initializing;
  }

  class Renderer {
    #native;

    constructor(options = {}) {
      if (!initialized) throw new Error(notInitialized);
      if (options === null || typeof options !== "object" || Array.isArray(options)) {
        throw new TypeError("renderer options must be an object");
      }
      for (const key of Object.keys(options)) {
        if (!new Set(["scale", "theme", "chrome"]).has(key)) {
          throw new TypeError(`unknown field ${JSON.stringify(key)} in renderer options`);
        }
      }
      this.#native = new NativeRenderer(options);
    }

    free() {
      this.#native.free();
    }

    normalizeGrid(grid) {
      return this.#native.normalizeGrid(grid);
    }

    gridToPng(grid) {
      return this.#native.gridToPng(grid);
    }

    gridToSvg(grid) {
      return this.#native.gridToSvg(grid);
    }

    gridToText(grid) {
      return this.#native.gridToText(grid);
    }

    gridToJson(grid) {
      return this.#native.gridToJson(grid);
    }

    ansiToGrid(ansi, options) {
      const [cols, rows] = dimensions(options);
      return this.#native.ansiToGrid(ansi, cols, rows);
    }

    ansiToPng(ansi, options) {
      const [cols, rows] = dimensions(options);
      return this.#native.ansiToPng(ansi, cols, rows);
    }

    ansiToSvg(ansi, options) {
      const [cols, rows] = dimensions(options);
      return this.#native.ansiToSvg(ansi, cols, rows);
    }

    ansiToText(ansi, options) {
      const [cols, rows] = dimensions(options);
      return this.#native.ansiToText(ansi, cols, rows);
    }

    ansiToJson(ansi, options) {
      const [cols, rows] = dimensions(options);
      return this.#native.ansiToJson(ansi, cols, rows);
    }

    renderGrid(input) {
      const json = this.normalizeGrid(input);
      return {
        png: this.gridToPng(json),
        svg: this.gridToSvg(json),
        txt: this.gridToText(json),
        json,
      };
    }

    renderAnsi(ansi, options) {
      const json = this.ansiToGrid(ansi, options);
      return {
        png: this.gridToPng(json),
        svg: this.gridToSvg(json),
        txt: this.gridToText(json),
        json,
      };
    }
  }

  async function usingRenderer(options, operation) {
    await init();
    const renderer = new Renderer(options);
    try {
      return operation(renderer);
    } finally {
      renderer.free();
    }
  }

  return Object.freeze({
    init,
    Renderer,
    version() {
      if (!initialized) throw new Error(notInitialized);
      return nativeVersion();
    },
    renderGrid(grid, options) {
      return usingRenderer(options, (renderer) => renderer.renderGrid(grid));
    },
    renderAnsi(ansi, options) {
      return usingRenderer(rendererOptions(options), (renderer) => renderer.renderAnsi(ansi, options));
    },
    gridToPng(grid, options) {
      return usingRenderer(options, (renderer) => renderer.gridToPng(grid));
    },
    gridToSvg(grid, options) {
      return usingRenderer(options, (renderer) => renderer.gridToSvg(grid));
    },
    gridToText(grid, options) {
      return usingRenderer(options, (renderer) => renderer.gridToText(grid));
    },
    gridToJson(grid, options) {
      return usingRenderer(options, (renderer) => renderer.gridToJson(grid));
    },
    ansiToGrid(ansi, options) {
      return usingRenderer(rendererOptions(options), (renderer) => renderer.ansiToGrid(ansi, options));
    },
    ansiToPng(ansi, options) {
      return usingRenderer(rendererOptions(options), (renderer) => renderer.ansiToPng(ansi, options));
    },
    ansiToSvg(ansi, options) {
      return usingRenderer(rendererOptions(options), (renderer) => renderer.ansiToSvg(ansi, options));
    },
    ansiToText(ansi, options) {
      return usingRenderer(rendererOptions(options), (renderer) => renderer.ansiToText(ansi, options));
    },
    ansiToJson(ansi, options) {
      return usingRenderer(rendererOptions(options), (renderer) => renderer.ansiToJson(ansi, options));
    },
  });
}
