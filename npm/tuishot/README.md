# tuishot

Unscoped alias for [`@tuishot/cli`](https://www.npmjs.com/package/@tuishot/cli).

```sh
npm install --save-dev tuishot
tuishot --size 120x40 --wait-text Ready -o dashboard.png -- my-tui
```

Both ESM and CommonJS consumers can use the typed API:

```js
import { shoot, runScenes } from "tuishot";
```

See the main [tuishot repository](https://github.com/junhoyeo/tuishot) for CLI and scene-file documentation.
