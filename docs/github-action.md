# GitHub Action

The ttyshot composite action runs a committed scene and opens a pull request only when selected screenshots change. The action is Linux-only while this repository is private.

> [!WARNING]
> Pull requests, action logs, and terminal screenshots can become public. Use synthetic fixture data only. Never capture personal usage, home-directory paths, credentials, customer data, or other private terminal output. A ttyshot timeout can include the last terminal screen in its error.

## Consumer workflow

The job needs write access to repository contents and pull requests:

```yaml
name: Refresh terminal screenshots

on:
  workflow_dispatch:
  schedule:
    - cron: "17 3 * * 1"

permissions:
  contents: write
  pull-requests: write

jobs:
  screenshots:
    # ttyshot's v0 action supports Linux runners only.
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v4
      - uses: junhoyeo/ttyshot@v0
        with:
          scenes: fixtures/readme/ttyshot.toml
          paths: |
            docs/screenshots
            README-assets/terminal overview.png
          branch: ttyshot/update-screenshots
          title: "docs: refresh terminal screenshots"
```

The full example is in [`examples/github-action.yml`](../examples/github-action.yml).

The action changes to its own checkout at `GITHUB_ACTION_PATH` before building. It uses a fresh Cargo home, an explicit Rust host target, a separate target directory under `RUNNER_TEMP`, and removes Rust wrapper/flag environment variables. Consumer `.cargo/config` files and `RUSTC_WRAPPER` settings therefore do not cross the build boundary. It then runs exactly:

```text
ttyshot run <scene-file>
```

from the checked-out consumer repository. It does not use a globally installed `ttyshot` or code from generated output.

## Inputs

| Input | Required | Default | Description |
| --- | --- | --- | --- |
| `scenes` | yes | — | One scene file relative to the consumer repository. |
| `paths` | yes | — | Newline-separated repository files or directories that may change. Spaces are preserved. |
| `branch` | no | `ttyshot/update-screenshots` | Dedicated update branch owned by this action. |
| `title` | no | `docs: refresh terminal screenshots` | Commit subject and pull request title. |
| `body` | no | generated body | Pull request body. The action always passes it to `gh` with `--body-file`. |
| `base` | no | repository default branch | Pull request base branch. |
| `token` | no | `${{ github.token }}` | Token used for this repository's push and pull request. |

Outputs:

- `changed`: `true` once an allowed generated commit is present on the dedicated remote branch. It is `false` only after a successful no-change run.
- `pr-url`: the reused or newly created pull request URL. It is empty when nothing changed. It also remains empty if the PR API fails after a successful push.

## Trusted-ref and credential model

The action accepts only `push`, `schedule`, and `workflow_dispatch`. It rejects `pull_request` and `pull_request_target` before building or generating anything. Those events can select or expose attacker-controlled repository content and are outside this action's trust model.

The checked-out `HEAD` must equal the freshly fetched tip of the selected base branch before the scene runs. This means a scheduled or manually dispatched workflow must check out the base branch, not a pull-request merge ref, detached historical revision, or prior update branch. The action records that base object ID and fails if the remote base moves before commit/push. Every generated commit has that exact object as its only parent.

Before scene execution, the helper removes checkout-injected `http.*.extraheader` values and saves the exact sanitized repository-local config bytes. It runs the scene with both `GH_TOKEN` and `GITHUB_TOKEN` absent. Immediately after the scene, it restores that config before running Git again. This removes scene-added URL rewrites, proxies, transports, push URLs, filters, and hook settings. The token is never written to `.git/config`, a credential helper, or a remote URL. Each authenticated Git operation receives a process-local Basic `extraheader` derived from `GH_TOKEN`; credential helpers, replacement objects, global/system config, and hooks are disabled for that operation. The scene therefore cannot read the token or inherit GitHub's checkout authorization header.

`GITHUB_REPOSITORY` is mandatory. PR lookup is scoped to that exact repository, its owner plus the configured update branch, the selected base, and the exact pushed head object ID. An open PR from a fork with the same branch name is not reused.

## Safety contract

The helper enforces these rules before it commits or contacts the PR API:

1. The workflow event must be trusted, and the consumer checkout must be clean.
2. `scenes` and each `paths` entry must be non-empty, relative, normalized repository paths. Absolute paths, `.git`, and `..` segments are rejected.
3. The action fetches the remote base before scene execution and requires checkout `HEAD` to equal it exactly. It also observes the remote update-branch object ID before the scene.
4. Worktree and index inventories always disable replacement objects, rename detection, external diffs, and submodule ignoring. They use NUL-delimited names. A rename from a disallowed path into an allowed path is two changes and is rejected.
5. A scene failure or an unexpected path stops before any action commit or push. Diagnostics shell-quote unsafe path bytes instead of printing control characters raw.
6. Only validated paths are staged, one by one, with `GIT_LITERAL_PATHSPECS=1`. A second invariant audit runs after staging. The action never uses a repository-wide stage command.
7. Before any post-scene Git command, the action restores the sanitized local config byte-for-byte, then restores and verifies the original `origin`. This defeats `url.*.insteadOf`, push URL, proxy, transport, diff, and hook mutations. Replacement refs cannot affect inspection. Empty controlled hook directories apply to commits and pushes.
8. Immediately before commit/push, it fetches the base again, checks `HEAD`, checks the base object, and checks the update branch against the pre-scene observations. A moved base or update branch fails closed.
9. If there are no changes, the action creates no commit, branch, or pull request and only then writes `changed=false`.

The action does not merge pull requests, publish packages, change repository visibility, or stage paths outside the explicit allowlist.

## Branch ownership, leases, and reruns

Treat `branch` as an automation-owned branch. Do not add human commits to it and do not share it with another updater. Use a different branch name if that branch already belongs to a person or another tool.

Before generation, the action reads the branch's remote object ID with replacement objects disabled and fetches that real object. The bot author plus `Generated-by: ttyshot GitHub Action` marker is only an accidental-overwrite guard. It is not authentication or branch identity. The object ID observed from the remote is the actual ownership boundary.

A push uses `--force-with-lease=<ref>:<pre-scene-oid>`, including an empty expected value when the branch did not exist. The helper rechecks branch movement before pushing, and the lease closes the final race. Scene changes to `origin`, push URLs, config, replacement refs, or hooks cannot redirect or forge the inspected branch.

Each update commit is rebuilt directly on the recorded base revision. This removes obsolete generated files from earlier runs and makes reruns converge. If the observed action branch already has the same parent, tree, and title, the action reuses that exact commit without pushing.

After a successful push or exact branch reuse, the helper writes `changed=true` and an empty `pr-url` before querying GitHub. If PR lookup or creation fails, the dedicated branch intentionally remains. A later run can converge on and reuse it. Permission errors name `contents: write`, `pull-requests: write`, and fork policy as applicable.

## Local deterministic tests

The shell helper is tested without Docker or GitHub. The suite creates temporary local repositories and a local bare remote, then supplies fake `ttyshot` and `gh` executables:

```bash
bash scripts/test-github-action.sh
```

It covers trusted-event rejection, exact base freshness, base and update-branch movement, rename exposure with local `diff.renames=true`, literal pathspec staging, spaces and control characters, token/header isolation, remote/config/hook/replace attacks, branch ownership, force-with-lease failures, exact-repository PR reuse, same-name fork PR rejection, body-file handling, and PR API failures after a successful push. A static build-boundary regression checks the isolated Cargo invocation.
