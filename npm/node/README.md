# @tuishot/node

In-process Node-API rendering for [tuishot](https://github.com/junhoyeo/tuishot).
It parses ANSI with the same Alacritty terminal model as the native CLI and can
render an existing validated tuishot grid. It never starts the CLI or a PTY.

```js
import { renderAnsi } from "@tuishot/node";

const { png, svg, txt, json } = renderAnsi("\x1b[31mhello\x1b[0m\nworld", {
  cols: 40,
  rows: 4,
});
```

`renderAnsi` treats bare LF as a newline at column zero, matching static output
from Ink's `renderToString`. Existing CRLF is preserved. Rendering uses the
bundled JetBrains Mono font only, so repeated output is byte-stable across
supported hosts. Static ANSI dimensions are limited to 1,000,000 cells to
bound in-process allocation before parsing.

The package supports Node.js 18 and newer. Native addons are installed through
one of eight platform-specific optional packages. To bound native allocations,
ANSI and Grid inputs are limited to 1,000,000 cells; Grid symbol text is limited
to 16 MiB in aggregate.

Licensed under MIT.
