# tuishot

Reserved source for an unpublished alias of [`@tuishot/cli`](https://www.npmjs.com/package/@tuishot/cli). npm rejected this name as too similar to `teashot`; this workspace package is private until the name is approved. Use `@tuishot/cli` instead.

```sh
npm install --save-dev tuishot
tuishot --size 120x40 --wait-text Ready -o dashboard.png -- my-tui
```

Both ESM and CommonJS consumers can use the typed API:

```js
import { shoot, runScenes } from "tuishot";
```

See the main [tuishot repository](https://github.com/junhoyeo/tuishot) for CLI and scene-file documentation.
