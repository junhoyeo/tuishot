# Ink `renderToString` example

This separate example demonstrates `@tuishot/node` with Ink's static
`renderToString` output. Ink 8 and Vitest 5 require Node.js 22.13 or newer, so
this directory has its own package manifest, lockfile, and CI lane. The
`@tuishot/node` package itself remains compatible with Node.js 18.

From the repository root, build the host addon with Node 22+, then run:

```sh
npm --prefix examples/ink ci
npm --prefix examples/ink test
```
