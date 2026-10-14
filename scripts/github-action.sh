#!/usr/bin/env bash
set -euo pipefail

readonly BOT_NAME='github-actions[bot]'
readonly BOT_EMAIL='41898282+github-actions[bot]@users.noreply.github.com'
readonly COMMIT_MARKER='Generated-by: ttyshot GitHub Action'

fail() {
  printf 'ttyshot action: %s\n' "$*" >&2
  exit 1
}

normalize_repo_path() {
  local input="$1"
  local part joined=''
  local -a parts normalized

  [[ -n "$input" ]] || return 1
  [[ "$input" != /* ]] || return 1
  [[ "$input" != *$'\n'* && "$input" != *$'\r'* ]] || return 1

  IFS='/' read -r -a parts <<< "$input"
  normalized=()
  for part in "${parts[@]}"; do
    case "$part" in
      ''|.) continue ;;
      ..|.git) return 1 ;;
      *) normalized+=("$part") ;;
    esac
  done
  ((${#normalized[@]} > 0)) || return 1

  for part in "${normalized[@]}"; do
    if [[ -z "$joined" ]]; then
      joined="$part"
    else
      joined="$joined/$part"
    fi
  done
  NORMALIZED_PATH="$joined"
}

path_is_allowed() {
  local candidate="$1"
  local allowed
  for allowed in "${ALLOWED_PATHS[@]}"; do
    if [[ "$candidate" == "$allowed" || "$candidate" == "$allowed/"* ]]; then
      return 0
    fi
  done
  return 1
}

write_outputs() {
  local changed="$1"
  local pr_url="$2"
  printf 'changed=%s\n' "$changed" >> "$GITHUB_OUTPUT"
  printf 'pr-url=%s\n' "$pr_url" >> "$GITHUB_OUTPUT"
}

: "${TTYSHOT_BIN:?TTYSHOT_BIN is required}"
: "${TTYSHOT_SCENES:?TTYSHOT_SCENES is required}"
: "${TTYSHOT_ALLOWED_PATHS:?TTYSHOT_ALLOWED_PATHS is required}"
: "${TTYSHOT_UPDATE_BRANCH:=ttyshot/update-screenshots}"
: "${TTYSHOT_UPDATE_TITLE:=docs: refresh terminal screenshots}"
: "${TTYSHOT_PR_BODY:=}"
: "${TTYSHOT_BASE_BRANCH:=}"
: "${TTYSHOT_DEFAULT_BASE:=}"
: "${GH_TOKEN:?A GitHub token is required; grant contents: write and pull-requests: write}"
: "${GITHUB_OUTPUT:?GITHUB_OUTPUT is required}"

command -v git >/dev/null || fail 'git is required'
command -v gh >/dev/null || fail 'gh is required'
[[ -x "$TTYSHOT_BIN" ]] || fail "ttyshot executable is missing: $TTYSHOT_BIN"
git rev-parse --is-inside-work-tree >/dev/null 2>&1 || fail 'run this action after actions/checkout'
[[ -n "$TTYSHOT_UPDATE_TITLE" ]] || fail 'title must not be empty'
git check-ref-format --branch "$TTYSHOT_UPDATE_BRANCH" >/dev/null 2>&1 || \
  fail "invalid update branch: $TTYSHOT_UPDATE_BRANCH"

if ! normalize_repo_path "$TTYSHOT_SCENES"; then
  fail 'scenes must be a non-empty relative repository path without .git or .. segments'
fi
readonly SCENE_PATH="$NORMALIZED_PATH"
[[ -f "$SCENE_PATH" ]] || fail "scene file does not exist: $SCENE_PATH"

ALLOWED_PATHS=()
while IFS= read -r allowed_path || [[ -n "$allowed_path" ]]; do
  if ! normalize_repo_path "$allowed_path"; then
    fail 'each allowed path must be non-empty and relative, without .git or .. segments'
  fi
  normalized_allowed="$NORMALIZED_PATH"
  duplicate=false
  for existing in "${ALLOWED_PATHS[@]}"; do
    if [[ "$existing" == "$normalized_allowed" ]]; then
      duplicate=true
      break
    fi
  done
  if [[ "$duplicate" == false ]]; then
    ALLOWED_PATHS+=("$normalized_allowed")
  fi
done <<< "$TTYSHOT_ALLOWED_PATHS"
((${#ALLOWED_PATHS[@]} > 0)) || fail 'at least one allowed path is required'

work_tmp="$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/ttyshot-action.XXXXXX")"
trap 'rm -rf "$work_tmp"' EXIT

# A dirty checkout can mix human edits into the generated commit, so refuse it.
git status --porcelain=v1 -z --untracked-files=all > "$work_tmp/status-before"
[[ ! -s "$work_tmp/status-before" ]] || fail 'checkout must be clean before scene generation'

write_outputs false ''

# ttyshot is expected to replace each output atomically. Even if it exits after
# creating a partial output, no Git mutation or network operation occurs below.
"$TTYSHOT_BIN" run "$SCENE_PATH" || fail 'scene generation failed; no commit or push was attempted'

CHANGED_PATHS=()
while IFS= read -r -d '' changed_path; do
  CHANGED_PATHS+=("$changed_path")
done < <(git diff --name-only -z --)
while IFS= read -r -d '' changed_path; do
  CHANGED_PATHS+=("$changed_path")
done < <(git diff --cached --name-only -z --)
while IFS= read -r -d '' changed_path; do
  CHANGED_PATHS+=("$changed_path")
done < <(git ls-files --others --exclude-standard -z --)

if ((${#CHANGED_PATHS[@]} == 0)); then
  printf 'ttyshot action: screenshots are already current\n'
  exit 0
fi

for changed_path in "${CHANGED_PATHS[@]}"; do
  path_is_allowed "$changed_path" || \
    fail "scene changed a path outside the allowlist: $changed_path"
done

base_branch="$TTYSHOT_BASE_BRANCH"
if [[ -z "$base_branch" ]]; then
  base_branch="$TTYSHOT_DEFAULT_BASE"
fi
if [[ -z "$base_branch" ]]; then
  base_branch="$(git symbolic-ref --quiet --short refs/remotes/origin/HEAD 2>/dev/null || true)"
  base_branch="${base_branch#origin/}"
fi
[[ -n "$base_branch" ]] || fail 'could not determine the base branch; set the base input'
git check-ref-format --branch "$base_branch" >/dev/null 2>&1 || fail "invalid base branch: $base_branch"

readonly update_ref="refs/heads/$TTYSHOT_UPDATE_BRANCH"
origin_url="$(git remote get-url origin)"
if [[ "$origin_url" == https://github.com/* ]]; then
  # Use this action's token for Git as well as gh, without placing it in a URL
  # or command argument. The helper reads GH_TOKEN only when Git asks for it.
  git config --local --unset-all http.https://github.com/.extraheader >/dev/null 2>&1 || true
  git config --local credential.https://github.com.helper ''
  git config --local --add credential.https://github.com.helper '!gh auth git-credential'
fi
remote_oid="$(git ls-remote --heads origin "$update_ref" | awk 'NR == 1 { print $1 }')"
observed_ref='refs/ttyshot-action/observed'
if [[ -n "$remote_oid" ]]; then
  git fetch --no-tags origin "$update_ref:$observed_ref"
  fetched_oid="$(git rev-parse "$observed_ref")"
  [[ "$fetched_oid" == "$remote_oid" ]] || \
    fail 'the update branch moved while it was being inspected; rerun the action'
  remote_email="$(git show -s --format=%ae "$observed_ref")"
  remote_body="$(git show -s --format=%B "$observed_ref")"
  [[ "$remote_email" == "$BOT_EMAIL" && "$remote_body" == *"$COMMIT_MARKER"* ]] || \
    fail "remote branch '$TTYSHOT_UPDATE_BRANCH' is not owned by the ttyshot action; choose a dedicated branch"
fi

# Stage each validated changed path explicitly. Never stage the whole worktree.
for changed_path in "${CHANGED_PATHS[@]}"; do
  git add -- "$changed_path"
done

STAGED_PATHS=()
while IFS= read -r -d '' staged_path; do
  STAGED_PATHS+=("$staged_path")
done < <(git diff --cached --name-only -z --)
((${#STAGED_PATHS[@]} > 0)) || fail 'changes disappeared before staging'
for staged_path in "${STAGED_PATHS[@]}"; do
  path_is_allowed "$staged_path" || fail "refusing to commit unexpected staged path: $staged_path"
done

git config --local user.name "$BOT_NAME"
git config --local user.email "$BOT_EMAIL"

base_oid="$(git rev-parse HEAD)"
staged_tree="$(git write-tree)"
commit_oid=''
needs_push=true
if [[ -n "$remote_oid" ]]; then
  remote_tree="$(git show -s --format=%T "$observed_ref")"
  remote_parent="$(git show -s --format=%P "$observed_ref")"
  remote_subject="$(git show -s --format=%s "$observed_ref")"
  if [[ "$remote_tree" == "$staged_tree" && "$remote_parent" == "$base_oid" && \
        "$remote_subject" == "$TTYSHOT_UPDATE_TITLE" ]]; then
    commit_oid="$remote_oid"
    needs_push=false
    printf 'ttyshot action: the observed update branch already has the generated tree\n'
  fi
fi

if [[ "$needs_push" == true ]]; then
  git commit --no-verify -m "$TTYSHOT_UPDATE_TITLE" -m "$COMMIT_MARKER"
  commit_oid="$(git rev-parse HEAD)"
  if ! git push --force-with-lease="$update_ref:$remote_oid" origin "$commit_oid:$update_ref"; then
    fail 'push failed; the branch may have moved or the workflow needs contents: write permission'
  fi
fi

existing_pr="$(gh pr list \
  --state open \
  --base "$base_branch" \
  --head "$TTYSHOT_UPDATE_BRANCH" \
  --limit 1 \
  --json url \
  --jq '.[0].url // empty')" || \
  fail 'could not query pull requests; grant pull-requests: write permission'

if [[ -n "$existing_pr" ]]; then
  pr_url="$existing_pr"
  printf 'ttyshot action: reusing %s\n' "$pr_url"
else
  body_file="$work_tmp/pr-body.md"
  if [[ -n "$TTYSHOT_PR_BODY" ]]; then
    printf '%s\n' "$TTYSHOT_PR_BODY" > "$body_file"
  else
    cat > "$body_file" <<EOF
This pull request refreshes terminal screenshots from the committed ttyshot scene.

- Scene: \`$SCENE_PATH\`
- Allowed paths are limited by the workflow configuration.

Generated by the ttyshot GitHub Action.
EOF
  fi
  if ! pr_url="$(gh pr create \
    --base "$base_branch" \
    --head "$TTYSHOT_UPDATE_BRANCH" \
    --title "$TTYSHOT_UPDATE_TITLE" \
    --body-file "$body_file")"; then
    fail 'could not create the pull request; grant pull-requests: write permission'
  fi
fi

[[ "$pr_url" != *$'\n'* && "$pr_url" != *$'\r'* ]] || fail 'gh returned an invalid pull request URL'
write_outputs true "$pr_url"
