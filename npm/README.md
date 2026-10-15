# npm packaging

`npm/targets.json` is the committed source of truth for the eight native targets. Run `npm run sync:packages` after changing the Cargo workspace version or target matrix. The sync command updates the scoped CLI, alias, generated platform manifests, copied MIT licenses, runtime target table, and `package-lock.json`.

The private root package lists each generated platform directory as a local `file:` optional dependency. This is lock/bootstrap plumbing only. It lets `npm ci --omit=optional` work before the native packages have ever been published. The published `@tuishot/cli` manifest still depends on all eight native packages by exact registry version.

Generate one publishable native package after building its artifact:

```sh
node npm/scripts/generate-platform-package.mjs \
  --target aarch64-apple-darwin \
  --binary /path/to/tuishot
```

The generator replaces `npm/platforms/<target>`, copies the binary and MIT license, and writes its manifest from the target matrix and Cargo workspace version. Never publish the source placeholder directory before generating it with a real binary.
