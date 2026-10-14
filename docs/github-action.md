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

The action builds the `ttyshot` package from its own checkout at `GITHUB_ACTION_PATH`. Build output and the debug binary stay under `RUNNER_TEMP`. It then runs exactly:

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

- `changed`: `true` when allowed paths changed, otherwise `false`.
- `pr-url`: the reused or newly created pull request URL. It is empty when nothing changed.

## Safety contract

The helper enforces these rules before it commits or contacts GitHub:

1. The consumer checkout must be clean before scene generation.
2. `scenes` and each `paths` entry must be non-empty, relative, normalized repository paths. Absolute paths, `.git`, and `..` segments are rejected.
3. After the scene finishes, tracked and untracked changes are collected with NUL-delimited Git output. Every changed path must equal an allowed path or be below an allowed directory.
4. A scene failure or an unexpected path stops before any commit or push. Partial files can remain in the disposable runner checkout for diagnosis.
5. Only validated changed paths are passed to `git add -- <path>`. The action never uses a repository-wide stage command.
6. If there are no changes, the action creates no commit, branch, or pull request.
7. The bot identity is configured with repository-local Git config only, after validation.

The token is supplied through `GH_TOKEN` and is never echoed. The action does not merge pull requests, publish packages, or change repository visibility.

## Branch ownership and reruns

Treat `branch` as an automation-owned branch. Do not add human commits to it and do not share it with another updater. Use a different branch name if that branch already belongs to a person or another tool.

Before updating an existing remote branch, the action fetches its observed object ID. It refuses to replace the branch unless its tip was authored by `github-actions[bot]` and contains the ttyshot action marker. A push uses `--force-with-lease=<ref>:<observed-oid>`, including an empty lease when the branch does not exist. If another actor moves the branch after inspection, the push fails instead of overwriting that work.

Each update commit is rebuilt directly on the checked-out base revision. This removes obsolete generated files from earlier runs and makes reruns converge. If the remote action commit already has the same parent, tree, and title, the action reuses it without pushing. It also reuses an open pull request with the same head and base instead of creating a duplicate.

For scheduled runs, check out the base branch rather than the update branch. The workflow must use a token that can push branches and open pull requests. Fork policies and organization rules can still block those operations; failures name the required `contents: write` or `pull-requests: write` permission.

## Local deterministic tests

The shell helper is tested without Docker or GitHub. The suite creates temporary local repositories and a local bare remote, then supplies fake `ttyshot` and `gh` executables:

```bash
bash scripts/test-github-action.sh
```

It covers no-change behavior, tracked and untracked outputs, outside-path rejection, paths containing spaces, a failed scene, new pull request body-file handling, and reuse of an existing pull request.
