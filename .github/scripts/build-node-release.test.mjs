import assert from "node:assert/strict";
import test from "node:test";
import { addonLibraryName, buildEnvironment, main } from "./build-node-release.mjs";

test("Cargo library filenames cover all release platform families", () => {
  assert.equal(addonLibraryName("aarch64-apple-darwin"), "libtuishot_node.dylib");
  assert.equal(addonLibraryName("x86_64-apple-darwin"), "libtuishot_node.dylib");
  for (const arch of ["x86_64", "aarch64"]) {
    for (const libc of ["gnu", "musl"]) {
      assert.equal(addonLibraryName(`${arch}-unknown-linux-${libc}`), "libtuishot_node.so");
    }
    assert.equal(addonLibraryName(`${arch}-pc-windows-msvc`), "tuishot_node.dll");
  }
  assert.throws(() => addonLibraryName("../../unsafe"), /Unsupported/);
});

test("musl selects a dynamic CRT and preserves existing flags without mutating input", () => {
  const original = { RUSTFLAGS: "-C debuginfo=1", PATH: "/bin" };
  assert.deepEqual(buildEnvironment("aarch64-unknown-linux-musl", original), {
    RUSTFLAGS: "-C debuginfo=1 -C target-feature=-crt-static", PATH: "/bin",
  });
  assert.equal(original.RUSTFLAGS, "-C debuginfo=1");
  assert.equal(buildEnvironment("x86_64-unknown-linux-musl", {}).RUSTFLAGS, "-C target-feature=-crt-static");
});

test("encoded Rust flags retain Cargo precedence and are extended with a unit separator", () => {
  assert.equal(buildEnvironment("aarch64-unknown-linux-musl", {CARGO_ENCODED_RUSTFLAGS: "-C\x1fdebuginfo=0"}).CARGO_ENCODED_RUSTFLAGS,
    "-C\x1fdebuginfo=0\x1f-C\x1ftarget-feature=-crt-static");
});

test("non-musl environments stay unchanged", () => {
  const env = { RUSTFLAGS: "-C debuginfo=1" };
  assert.deepEqual(buildEnvironment("aarch64-apple-darwin", env), env);
  assert.deepEqual(buildEnvironment("x86_64-pc-windows-msvc", env), env);
});

test("invalid target arguments are rejected before any build process", () => {
  assert.throws(() => main([]), /Usage/);
  assert.throws(() => main(["--target", "../../unsafe"]), /Unsupported/);
});
