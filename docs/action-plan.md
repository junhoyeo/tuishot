# Composite action contract (v0 draft)

The launch action runs committed ttyshot scenes on Linux and opens or updates one pull request when selected screenshot paths change.

```yaml
- uses: junhoyeo/ttyshot@v0
  with:
    scenes: ttyshot.toml
    paths: screenshots
    branch: ttyshot/update-screenshots
    title: "docs: refresh terminal screenshots"
```

## Inputs

| Input | Required | Default | Meaning |
| --- | --- | --- | --- |
| `scenes` | yes | — | Scene file path relative to the checked-out consumer repository |
| `paths` | yes | — | Newline-separated repository paths that the action is allowed to stage |
| `branch` | no | `ttyshot/update-screenshots` | Dedicated update branch |
| `title` | no | `docs: refresh terminal screenshots` | Commit and PR title |
| `body` | no | generated | PR body; written through a file, never inline shell interpolation |
| `base` | no | current default branch | PR base branch |
| `token` | no | `${{ github.token }}` | Token used only for the current repository |

## Safety and behavior

1. Require a clean checkout before running scenes.
2. Build/install the action's pinned ttyshot source under `$RUNNER_TEMP`; do not execute code from generated fixture output.
3. Run `ttyshot run` once. A scene failure exits without committing partial files because ttyshot writes each output atomically.
4. Inspect changes only under the explicitly allowed `paths`. A change elsewhere is an error.
5. If allowed paths are unchanged, exit successfully without creating a branch or PR.
6. Configure `github-actions[bot]` only inside the disposable checkout, stage only allowed paths, commit, and push the dedicated branch with `--force-with-lease`.
7. Reuse the existing open PR for that head/base pair or create one with `gh pr create --body-file`.
8. Never print tokens or scene environment values. GitHub masks the token; ttyshot errors include the terminal screen, so public scenes must use synthetic data.

The action requires consumer workflow permissions `contents: write` and `pull-requests: write`. It never changes repository visibility, publishes a package, or merges the PR.

## CI tests before launch

- no-change run creates no commit
- PNG/SVG change stages only the allowed directory
- an unexpected changed path fails
- an existing update PR is reused
- a scene timeout produces no commit
- paths containing spaces are handled without word splitting
- fork/insufficient-permission errors explain required permissions
