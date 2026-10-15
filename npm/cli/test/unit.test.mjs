import assert from "node:assert/strict";
import { chmod, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { buildShootArgv } from "../dist/esm/api.js";
import { NativeCommandError, runNative } from "../dist/esm/native.js";
import { detectLibc, resolveTarget } from "../dist/esm/resolver.js";
import { TARGETS } from "../dist/esm/targets.generated.js";
import * as publicApi from "../dist/esm/index.js";

test("package root import is side-effect-free", () => {
  assert.equal(typeof publicApi.shoot, "function");
  assert.equal(typeof publicApi.runScenes, "function");
});

test("target matrix resolves exactly eight supported packages", () => {
  assert.equal(TARGETS.length, 8);
  assert.equal(new Set(TARGETS.map((target) => target.package)).size, 8);
  for (const target of TARGETS) {
    const resolved = resolveTarget({
      platform: target.platform,
      arch: target.arch,
      ...(target.platform === "linux" ? { libc: target.libc } : {}),
    });
    assert.equal(resolved.package, target.package);
  }
  assert.throws(
    () => resolveTarget({ platform: "freebsd", arch: "x64" }),
    /does not provide a native binary for freebsd\/x64.*supported targets:/,
  );
});

test("Linux libc detection uses report, ldd, and loaders without guessing", () => {
  assert.equal(
    detectLibc({ report: { header: { glibcVersionRuntime: "2.39" } }, runLdd: () => "" }),
    "gnu",
  );
  assert.equal(
    detectLibc({ report: { sharedObjects: ["/lib/ld-musl-x86_64.so.1"] }, runLdd: () => "" }),
    "musl",
  );
  assert.equal(detectLibc({ report: {}, runLdd: () => "musl libc (x86_64)" }), "musl");
  assert.equal(detectLibc({ report: {}, runLdd: () => "ldd (GNU libc) 2.36" }), "gnu");
  assert.equal(
    detectLibc({
      report: {},
      runLdd: () => "unknown",
      listDirectory: (directory) => (directory === "/lib" ? ["ld-musl-aarch64.so.1"] : []),
    }),
    "musl",
  );
  assert.equal(
    detectLibc({
      report: {},
      runLdd: () => "unknown",
      listDirectory: (directory) => (directory === "/lib64" ? ["ld-linux-x86-64.so.2"] : []),
    }),
    "gnu",
  );
  assert.equal(
    detectLibc({
      report: {},
      runLdd: () => "unknown",
      listDirectory: (directory) =>
        directory === "/lib" ? ["ld-musl-x86_64.so.1", "ld-linux-x86-64.so.2"] : [],
      fileExists: (path) => path === "/etc/alpine-release",
    }),
    "musl",
  );
  assert.throws(
    () =>
      detectLibc({
        report: {},
        runLdd: () => "unknown implementation",
        listDirectory: () => [],
        fileExists: () => false,
      }),
    /could not determine.*refusing to guess/,
  );
});

test("shoot argv uses native strings and equals-form flags", () => {
  const built = buildShootArgv(
    {
      command: "/bin/example",
      args: ["--child-flag", "space value"],
      size: "120x40",
      waitText: "Ready",
      waitRegex: "R.*y",
      waitStable: "300ms",
      keys: ["tab", "ctrl-a"],
      env: { ZED: "last", ALPHA: "a=b" },
      envClear: true,
      cwd: "/captured/cwd",
      home: "/captured/home",
      formats: ["png", "svg", "txt", "json", "png"],
      scale: 2,
      chrome: "macos",
      timeout: "2m",
    },
    "/tmp/output base",
  );
  assert.deepEqual(built.formats, ["png", "svg", "txt", "json"]);
  assert.deepEqual(built.argv, [
    "--output=/tmp/output base",
    "--format=png,svg,txt,json",
    "--size=120x40",
    "--wait-text=Ready",
    "--wait-regex=R.*y",
    "--wait-stable=300ms",
    "--keys=tab,ctrl-a",
    "--env-clear",
    "--env=ALPHA=a=b",
    "--env=ZED=last",
    "--cwd=/captured/cwd",
    "--home=/captured/home",
    "--scale=2",
    "--chrome=macos",
    "--timeout=2m",
    "--",
    "/bin/example",
    "--child-flag",
    "space value",
  ]);
  assert.deepEqual(buildShootArgv({ command: "app" }, "/tmp/frame").formats, ["png"]);
  assert.throws(() => buildShootArgv({ command: "app", formats: [] }, "/tmp/frame"), /formats must not be empty/);
});

test("native failure preserves stderr, exit code, and signal", async (context) => {
  if (process.platform === "win32") {
    context.skip("POSIX executable fixture");
    return;
  }
  const directory = await mkdtemp(join(tmpdir(), "tuishot-native-error-"));
  try {
    const failure = join(directory, "failure.js");
    await writeFile(failure, "#!/usr/bin/env node\nprocess.stderr.write('precise stderr\\n'); process.exit(23);\n");
    await chmod(failure, 0o755);
    await assert.rejects(
      runNative(["one", "two"], { binary: failure }),
      (error) => {
        assert.ok(error instanceof NativeCommandError);
        assert.equal(error.code, 23);
        assert.equal(error.signal, null);
        assert.equal(error.stderr, "precise stderr");
        assert.match(error.message, /code 23/);
        assert.match(error.message, /stderr:\nprecise stderr/);
        return true;
      },
    );

    const signaled = join(directory, "signal.js");
    await writeFile(signaled, "#!/usr/bin/env node\nprocess.kill(process.pid, 'SIGTERM');\n");
    await chmod(signaled, 0o755);
    await assert.rejects(
      runNative([], { binary: signaled }),
      (error) => {
        assert.ok(error instanceof NativeCommandError);
        assert.equal(error.code, null);
        assert.equal(error.signal, "SIGTERM");
        assert.match(error.message, /signal SIGTERM/);
        return true;
      },
    );
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
