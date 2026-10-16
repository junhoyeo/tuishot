# Releasing tuishot

Everything a release publishes comes from one tagged commit. The Cargo
workspace version is the single source of truth, a `v*` tag starts the release,
and `.github/workflows/release.yml` does the rest.

Nothing publishes on a normal branch push or pull request. Only a `v*` tag
can publish. A manual run on a branch is a dry run; a manual run targeting
a matching version tag can publish only with `dry_run: false`.

## Current registry state

Version 0.2.0 was published for the six Rust crates and sixteen scoped npm packages.
The unscoped `tuishot` alias was rejected by npm as too similar to `teashot`.
It remains a private workspace package and is excluded from publishing. Install
`@tuishot/cli` instead; the command is still named `tuishot`.

Version 0.2.1 corrects the executable permissions lost during CLI artifact transfer.
The release workflow now restores packages after downloading and exercises an
actual transported CLI package before any publishing job can start.

`@tuishot/wasm` is deferred until its parser-parity checks pass.

## What the release contains

| Artifact | Count | Notes |
| --- | --- | --- |
| GitHub Release archives | 6 | `.tar.gz` plus a `.sha256`, and one combined `SHA256SUMS` |
| npm packages | 16 | Scoped CLI and addon packages; unscoped alias and WASM are not published |
| crates.io crates | 6 | `tuishot-core`, `tuishot-render`, `tuishot-ansi`, `tuishot-term`, `tuishot-ratatui`, `tuishot` |

### Target support

The CLI and the Node-API addon do not cover the same targets, because they do
not use the same code.

| Rust target | `tuishot` CLI | `@tuishot/node` addon |
| --- | --- | --- |
| `aarch64-apple-darwin` | yes | yes |
| `x86_64-apple-darwin` | yes | yes |
| `x86_64-unknown-linux-gnu` | yes | yes |
| `x86_64-unknown-linux-musl` | yes | yes |
| `aarch64-unknown-linux-gnu` | yes | yes |
| `aarch64-unknown-linux-musl` | yes | yes |
| `x86_64-pc-windows-msvc` | **no** | yes |
| `aarch64-pc-windows-msvc` | **no** | yes |

**There is no Windows CLI build.** `tuishot-term` drives a real PTY and
declares `nix` as an unconditional dependency, so
`cargo check -p tuishot --target x86_64-pc-windows-msvc` fails to compile. The
Node-API addon never touches the PTY driver, so it does build for
`windows-msvc` and keeps both `@tuishot/node-win32-*` packages.

A Windows ConPTY backend for `tuishot-term` is deferred future work, queued as a
v0.3 candidate. It is a real feature with its own design and tests, so it is not
rushed into the launch release.

The two win32 entries in `npm/targets.json` carry `"cli": false`. That field is
what keeps the gap honest:

- `npm/scripts/generate-platform-package.mjs` refuses those targets, so no empty
  `@tuishot/cli-win32-*` package can be produced.
- `@tuishot/cli` lists only the 6 optional dependencies that really exist.
- `npm/scripts/check-release-manifest.mjs` compares the build matrices and the
  published package lists in `release.yml` against `npm/targets.json`. If the
  two ever drift, the release fails instead of shipping a broken install. CI
  runs the same check (`npm run check:release`) on every pull request.

## Release procedure

### 1. Bump the version

The Cargo workspace version is the only place a version is written by hand.

```sh
# Edit [workspace.package].version in Cargo.toml, for example to 0.2.0.
npm run sync:packages
```

`npm run sync:packages` rewrites every npm manifest, the generated platform
manifests, `npm/cli/src/targets.generated.ts`, the embedded loader versions, and
`package-lock.json` from that one value.

### 2. Verify the tip you are about to tag

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features --locked
npm ci --omit=optional
npm run check:packages     # Cargo/npm version and lockfile coherence
npm run check:release      # release.yml matches npm/targets.json
npm run test:npm
npm run test:npm:release-generation
npm run test:node:release-generation
npm run pack:npm:dry-run
npm run pack:node:dry-run
```

A dry run of the workflow itself is also available: run the **Release** workflow
from the Actions tab with `dry_run` left at `true`. It builds and packs every
artifact and publishes nothing.

### 3. Commit, tag, and push

```sh
git commit -m "chore: release v0.2.0"
git tag v0.2.0
git push origin main
git push origin v0.2.0
```

The tag must equal `v` plus the Cargo workspace version. The `plan` job compares
them and stops the release on a mismatch, so a stale `npm run sync:packages` can
never publish mixed versions.

### 4. What the workflow does

1. **plan** resolves the version from the tag, then runs `check:release` and
   `check:packages`.
2. **build-cli** builds `tuishot` for the 6 Unix targets, runs
   `tuishot --version` on each binary, makes a `.tar.gz` with a `.sha256`, and
   generates the `@tuishot/cli-*` platform package.
3. **build-node** uses Cargo directly for all 8 addon targets, preserves the
   committed JavaScript/type definitions, and loads each packaged addon to
   verify PNG/SVG/text/JSON rendering. The musl targets build and run in a
   digest-pinned Alpine image so the GCC runtime matches the host libc.
   It then uploads the `@tuishot/node-*` platform package.
4. **build-wasm** builds `@tuishot/wasm` when the commit provides a `build:wasm`
   script.
5. **github-release** collects every archive, writes one combined
   `tuishot-<version>-SHA256SUMS`, verifies it with `sha256sum --check
   --strict`, and creates the GitHub Release with `--verify-tag`.
6. **npm publish** runs in dependency order, because a platform package must
   exist on the registry before the package that lists it as an optional
   dependency:
   1. the 6 `@tuishot/cli-*` platform packages
   2. `@tuishot/cli` (the unscoped alias is blocked by npm and is not published)
   3. the 8 `@tuishot/node-*` platform packages
   4. `@tuishot/node`
   5. `@tuishot/wasm`
7. **publish-crates** publishes each crate in dependency order and waits for
   crates.io to index each version before the next crate resolves it.

## One-time setup

### npm trusted publisher settings

Trusted publishing lets the workflow get a short-lived npm credential from
GitHub's OIDC token, so no long-lived npm token is stored in the repository. The
workflow requests `id-token: write`, installs npm `>= 11.5.1` on Node `>= 22.14`,
and runs `npm publish --provenance --access public`.

Enter these settings **once per package**, at
`https://www.npmjs.com/package/<name>/access`, under **Trusted publisher**:

| Field | Value |
| --- | --- |
| Select your publisher | GitHub Actions |
| Organization or user | `junhoyeo` |
| Repository | `tuishot` |
| Workflow filename | `release.yml` |
| Environment name | *(leave empty)* |
| Allowed actions | allow `npm publish` |

npm does not validate these settings when you save them. A wrong value only
shows up as a failure during the next publish.

The workflow filename is always `release.yml`. Enter only the filename, not a
path, and keep the `.yml` extension.

Every `npm publish` runs directly inside `release.yml`, and that is deliberate.
npm validates the OIDC claim against the **calling** workflow's filename, and
npm's own troubleshooting notes warn that a `workflow_call` indirection "can
cause configuration mismatches". That gap is still open upstream as
[npm/documentation#1755][npm-1755]. Do not move the publish steps into a
reusable workflow, even to remove duplication, or trusted publishing will break
for all 18 packages.

npm also requires that each package's `repository.url` matches this GitHub
repository, which `npm run sync:packages` already keeps correct.

[npm-1755]: https://github.com/npm/documentation/issues/1755

All sixteen published scoped packages need the same settings. Configure WASM only after its later first publication:

**CLI, 7 packages**

1. `@tuishot/cli`
2. `@tuishot/cli-darwin-arm64`
3. `@tuishot/cli-darwin-x64`
4. `@tuishot/cli-linux-x64-gnu`
5. `@tuishot/cli-linux-x64-musl`
6. `@tuishot/cli-linux-arm64-gnu`
7. `@tuishot/cli-linux-arm64-musl`

**Node addon, 9 packages**

8. `@tuishot/node`
9. `@tuishot/node-darwin-arm64`
10. `@tuishot/node-darwin-x64`
11. `@tuishot/node-linux-x64-gnu`
12. `@tuishot/node-linux-x64-musl`
13. `@tuishot/node-linux-arm64-gnu`
14. `@tuishot/node-linux-arm64-musl`
15. `@tuishot/node-win32-x64-msvc`
16. `@tuishot/node-win32-arm64-msvc`

**Deferred:** `@tuishot/wasm`. Do not configure or publish it until the WASM implementation is ready. The unscoped `tuishot` name remains blocked by npm.

There are no `@tuishot/cli-win32-x64-msvc` or `@tuishot/cli-win32-arm64-msvc`
packages. Do not create them.

### crates.io trusted publishing

Do this once per crate, at
`https://crates.io/crates/<name>/settings/trusted-publishing`:

| Field | Value |
| --- | --- |
| Repository owner | `junhoyeo` |
| Repository name | `tuishot` |
| Workflow filename | `release.yml` |
| Environment | *(leave empty)* |

Configure all 6 published crates: `tuishot-core`, `tuishot-render`,
`tuishot-ansi`, `tuishot-term`, `tuishot-ratatui`, `tuishot`.

`tuishot-node` is not published; it carries `publish = false` and exists only to
build the `@tuishot/node` addon. `tuishot-wasm` is likewise unpublished.

> `tuishot-ansi` is a new crate that does not exist on crates.io yet, so its
> trusted-publishing settings can only be added after its first publish. Use the
> token fallback below for that first version.

**Token fallback.** If trusted publishing is unavailable, store an API token in
the `CARGO_REGISTRY_TOKEN` repository secret. The `publish-crates` job tries
trusted publishing first and falls back to that secret, so no change to the
workflow is needed.

## First publish, and how to turn the fallback off

Configure a trusted publisher on each existing package before selecting OIDC.
The sixteen scoped packages now exist, but package-level publisher setup may
still require an npm login/password confirmation. Bootstrap token publishing
is temporary; remove it after those settings are verified. Future new packages
such as WASM need their own first-publication setup.

1. Create a **granular access token** on npm with publish rights for the
   `@tuishot` scope and the `tuishot` package. An uncreated unscoped package
   cannot be selected by name, so bootstrap may require temporary all-package
   write access. Use the shortest usable expiry and revoke it after setup.
2. Store it as the repository secret `NPM_TOKEN`.
3. Push the release tag as usual. The automatic tag run will fail at the npm
   publish steps, because no package has a trusted publisher yet. That is
   expected.
4. Re-run the release from the Actions tab: choose the **Release** workflow,
   select the tag as the ref, and set
   - `dry_run`: `false`
   - `use_npm_token`: `true`
5. The run authenticates with `NPM_TOKEN` instead of OIDC. Provenance needs
   OIDC, so this first release publishes **without** provenance. Everything else
   is identical.

`dry_run` defaults to `true`, and it wins even on a tag. A manual run is
therefore a rehearsal unless you explicitly set `dry_run` to `false`.

`use_npm_token` defaults to `false` and exists only for this first publish. It
is a `workflow_dispatch` input, not an environment variable, so a tag push can
never reach the token path.

**Turn it off afterwards:**

1. Add the trusted publisher settings to all sixteen published scoped packages, as listed above.
2. Revoke the npm token on npm.
3. Delete the `NPM_TOKEN` repository secret.
4. Confirm the next release logs `Using OIDC trusted publishing with
   provenance.` and that each package page shows a provenance attestation.

After that, never set `use_npm_token` to `true` again. If a publish fails with
an OIDC error, fix the trusted publisher settings rather than falling back to a
token.

## Re-running a failed release

Every publish step is safe to re-run:

- `publish-crates` skips a crate whose version is already on crates.io.
- `github-release` uploads with `--clobber` when the release already exists.
- npm rejects republishing a version that already exists, so a partially
  finished npm group fails loudly instead of publishing something different
  under the same version.

A version that is already fully published cannot be replaced. Bump the version
and tag again.
