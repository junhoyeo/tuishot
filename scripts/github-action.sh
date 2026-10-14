#!/usr/bin/env bash
set -euo pipefail

readonly BOT_NAME='github-actions[bot]'
readonly BOT_EMAIL='41898282+github-actions[bot]@users.noreply.github.com'
readonly COMMIT_MARKER='Generated-by: ttyshot GitHub Action'

fail() {
  printf 'ttyshot action: %s\n' "$*" >&2
  exit 1
}

quote_path() {
  printf '%q' "$1"
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

write_no_change_outputs() {
  printf 'changed=false\n' >> "$GITHUB_OUTPUT"
  printf 'pr-url=\n' >> "$GITHUB_OUTPUT"
}

write_changed_outputs() {
  # Write this as soon as the generated branch exists remotely. If the PR API
  # fails, a convergent rerun can reuse the branch and pr-url remains empty.
  printf 'changed=true\n' >> "$GITHUB_OUTPUT"
  printf 'pr-url=\n' >> "$GITHUB_OUTPUT"
}

remove_checkout_extraheaders() {
  local key remaining
  local -a keys=()
  while IFS= read -r key; do
    [[ -n "$key" ]] && keys+=("$key")
  done < <(git --no-replace-objects config --local --name-only --get-regexp '^http\..*\.extraheader$' 2>/dev/null || true)
  for key in "${keys[@]}"; do
    git --no-replace-objects config --local --unset-all "$key" >/dev/null 2>&1 || true
  done
  remaining="$(git --no-replace-objects config --local --name-only --get-regexp '^http\..*\.extraheader$' 2>/dev/null || true)"
  [[ -z "$remaining" ]] || fail 'could not remove all checkout HTTP extraheaders'
}

reassert_origin() {
  local fetch_urls push_urls
  # A scene is untrusted generated-content code. Recreate the remote section so
  # it cannot redirect fetch/push with a changed URL or pushurl.
  git --no-replace-objects config --local --remove-section remote.origin >/dev/null 2>&1 || true
  git --no-replace-objects remote add origin "$ORIGIN_URL" || \
    fail 'scene changed Git config so origin could not be restored safely'
  remove_checkout_extraheaders
  fetch_urls="$(git --no-replace-objects remote get-url --all origin)"
  push_urls="$(git --no-replace-objects remote get-url --push --all origin)"
  [[ "$fetch_urls" == "$ORIGIN_URL" && "$push_urls" == "$ORIGIN_URL" ]] || \
    fail 'scene changed Git config so origin could not be restored exactly'
}

git_network() {
  local encoded header
  encoded="$(printf 'x-access-token:%s' "$ACTION_GH_TOKEN" | base64 | tr -d '\r\n')"
  header="AUTHORIZATION: basic $encoded"
  env -u GH_TOKEN -u GITHUB_TOKEN \
    GIT_TERMINAL_PROMPT=0 \
    GIT_NO_REPLACE_OBJECTS=1 \
    GIT_CONFIG_GLOBAL=/dev/null \
    GIT_CONFIG_SYSTEM=/dev/null \
    GIT_CONFIG_COUNT=5 \
    GIT_CONFIG_KEY_0=http.extraheader \
    GIT_CONFIG_VALUE_0= \
    GIT_CONFIG_KEY_1=http.https://github.com/.extraheader \
    GIT_CONFIG_VALUE_1="$header" \
    GIT_CONFIG_KEY_2=credential.helper \
    GIT_CONFIG_VALUE_2= \
    GIT_CONFIG_KEY_3=core.hooksPath \
    GIT_CONFIG_VALUE_3="$EMPTY_HOOKS" \
    GIT_CONFIG_KEY_4=push.gpgSign \
    GIT_CONFIG_VALUE_4=false \
    git --no-replace-objects "$@"
}

gh_network() {
  env -u GITHUB_TOKEN GH_TOKEN="$ACTION_GH_TOKEN" gh "$@"
}

fetch_base_oid() {
  git_network fetch --quiet --no-tags --force origin \
    "refs/heads/$BASE_BRANCH:$BASE_OBSERVED_REF" || \
    fail "could not fetch base branch '$BASE_BRANCH'; check the ref and contents permission"
  git --no-replace-objects rev-parse --verify "$BASE_OBSERVED_REF^{commit}"
}

read_remote_update_oid() {
  local listing
  if ! listing="$(git_network ls-remote --heads origin "$UPDATE_REF")"; then
    fail 'could not inspect the update branch; check repository access and contents permission'
  fi
  REMOTE_UPDATE_OID=''
  if [[ -n "$listing" ]]; then
    [[ "$listing" != *$'\n'* ]] || fail 'remote returned more than one update-branch ref'
    REMOTE_UPDATE_OID="${listing%%[[:space:]]*}"
    [[ "$REMOTE_UPDATE_OID" =~ ^[0-9a-fA-F]{40,64}$ ]] || fail 'remote returned an invalid update-branch object ID'
  fi
}

inventory_changes() {
  CHANGED_PATHS=()
  local changed_path existing already_seen
  while IFS= read -r -d '' changed_path; do
    already_seen=false
    for existing in "${CHANGED_PATHS[@]}"; do
      [[ "$existing" == "$changed_path" ]] && already_seen=true && break
    done
    [[ "$already_seen" == true ]] || CHANGED_PATHS+=("$changed_path")
  done < <(git --no-replace-objects -c diff.renames=false diff --no-ext-diff --no-renames --ignore-submodules=none --name-only -z --)
  while IFS= read -r -d '' changed_path; do
    already_seen=false
    for existing in "${CHANGED_PATHS[@]}"; do
      [[ "$existing" == "$changed_path" ]] && already_seen=true && break
    done
    [[ "$already_seen" == true ]] || CHANGED_PATHS+=("$changed_path")
  done < <(git --no-replace-objects -c diff.renames=false diff --cached --no-ext-diff --no-renames --ignore-submodules=none --name-only -z --)
  while IFS= read -r -d '' changed_path; do
    already_seen=false
    for existing in "${CHANGED_PATHS[@]}"; do
      [[ "$existing" == "$changed_path" ]] && already_seen=true && break
    done
    [[ "$already_seen" == true ]] || CHANGED_PATHS+=("$changed_path")
  done < <(git --no-replace-objects ls-files --others --exclude-standard -z --)
}

audit_allowed_changes() {
  local changed_path quoted
  inventory_changes
  for changed_path in "${CHANGED_PATHS[@]}"; do
    if ! path_is_allowed "$changed_path"; then
      quoted="$(quote_path "$changed_path")"
      fail "scene changed a path outside the allowlist: $quoted"
    fi
  done
}

# Pull-request events run attacker-controlled repository content. Reject them
# before checking the binary, repository, token, or scene.
: "${GITHUB_EVENT_NAME:?GITHUB_EVENT_NAME is required}"
case "$GITHUB_EVENT_NAME" in
  push|schedule|workflow_dispatch) ;;
  pull_request|pull_request_target)
    fail "event '$GITHUB_EVENT_NAME' is not trusted; use push, schedule, or workflow_dispatch"
    ;;
  *) fail "unsupported event '$GITHUB_EVENT_NAME'; use push, schedule, or workflow_dispatch" ;;
esac

: "${TTYSHOT_BIN:?TTYSHOT_BIN is required}"
: "${TTYSHOT_SCENES:?TTYSHOT_SCENES is required}"
: "${TTYSHOT_ALLOWED_PATHS:?TTYSHOT_ALLOWED_PATHS is required}"
: "${TTYSHOT_UPDATE_BRANCH:=ttyshot/update-screenshots}"
: "${TTYSHOT_UPDATE_TITLE:=docs: refresh terminal screenshots}"
: "${TTYSHOT_PR_BODY:=}"
: "${TTYSHOT_BASE_BRANCH:=}"
: "${TTYSHOT_DEFAULT_BASE:=}"
: "${GH_TOKEN:?A GitHub token is required; grant contents: write and pull-requests: write}"
# Keep the token out of the helper's exported environment while the scene
# runs. It is exposed only to the short-lived authenticated Git/gh children
# after generation. This is defense in depth inside the trusted-ref model.
ACTION_GH_TOKEN="$GH_TOKEN"
unset GH_TOKEN GITHUB_TOKEN
readonly ACTION_GH_TOKEN
: "${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is required}"
: "${GITHUB_OUTPUT:?GITHUB_OUTPUT is required}"

[[ "$GITHUB_REPOSITORY" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || \
  fail 'GITHUB_REPOSITORY must be an owner/repository name'
readonly REPOSITORY_OWNER="${GITHUB_REPOSITORY%%/*}"

command -v git >/dev/null || fail 'git is required'
command -v gh >/dev/null || fail 'gh is required'
command -v base64 >/dev/null || fail 'base64 is required'
[[ -x "$TTYSHOT_BIN" ]] || fail "ttyshot executable is missing: $(quote_path "$TTYSHOT_BIN")"
git --no-replace-objects rev-parse --is-inside-work-tree >/dev/null 2>&1 || fail 'run this action after actions/checkout'
[[ -n "$TTYSHOT_UPDATE_TITLE" ]] || fail 'title must not be empty'
git --no-replace-objects check-ref-format --branch "$TTYSHOT_UPDATE_BRANCH" >/dev/null 2>&1 || \
  fail "invalid update branch: $(quote_path "$TTYSHOT_UPDATE_BRANCH")"

if ! normalize_repo_path "$TTYSHOT_SCENES"; then
  fail 'scenes must be a non-empty relative repository path without .git or .. segments'
fi
readonly SCENE_PATH="$NORMALIZED_PATH"
[[ -f "$SCENE_PATH" ]] || fail "scene file does not exist: $(quote_path "$SCENE_PATH")"

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

BASE_BRANCH="$TTYSHOT_BASE_BRANCH"
if [[ -z "$BASE_BRANCH" ]]; then
  BASE_BRANCH="$TTYSHOT_DEFAULT_BASE"
fi
if [[ -z "$BASE_BRANCH" ]]; then
  BASE_BRANCH="$(git --no-replace-objects symbolic-ref --quiet --short refs/remotes/origin/HEAD 2>/dev/null || true)"
  BASE_BRANCH="${BASE_BRANCH#origin/}"
fi
[[ -n "$BASE_BRANCH" ]] || fail 'could not determine the base branch; set the base input'
git --no-replace-objects check-ref-format --branch "$BASE_BRANCH" >/dev/null 2>&1 || \
  fail "invalid base branch: $(quote_path "$BASE_BRANCH")"
readonly BASE_BRANCH
readonly UPDATE_REF="refs/heads/$TTYSHOT_UPDATE_BRANCH"
readonly BASE_OBSERVED_REF='refs/ttyshot-action/base'
readonly UPDATE_OBSERVED_REF='refs/ttyshot-action/update'

work_tmp="$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/ttyshot-action.XXXXXX")"
trap 'rm -rf "$work_tmp"' EXIT
EMPTY_HOOKS="$work_tmp/empty-hooks"
mkdir -p "$EMPTY_HOOKS"
readonly EMPTY_HOOKS

# A dirty checkout can mix human edits into the generated commit, so refuse it.
git --no-replace-objects status --porcelain=v1 -z --untracked-files=all > "$work_tmp/status-before"
[[ ! -s "$work_tmp/status-before" ]] || fail 'checkout must be clean before scene generation'

origin_count="$(git --no-replace-objects remote get-url --all origin | awk 'END { print NR }')"
[[ "$origin_count" == 1 ]] || fail 'origin must have exactly one fetch URL'
ORIGIN_URL="$(git --no-replace-objects remote get-url origin)"
readonly ORIGIN_URL
remove_checkout_extraheaders
LOCAL_CONFIG_PATH="$(git --no-replace-objects rev-parse --path-format=absolute --git-path config)"
[[ "$LOCAL_CONFIG_PATH" == /* && -f "$LOCAL_CONFIG_PATH" ]] || \
  fail 'could not locate the repository-local Git config'
readonly LOCAL_CONFIG_PATH
SANITIZED_CONFIG_SNAPSHOT="$work_tmp/local-config.before-scene"
cp -p -- "$LOCAL_CONFIG_PATH" "$SANITIZED_CONFIG_SNAPSHOT"
readonly SANITIZED_CONFIG_SNAPSHOT

# Establish both the exact base and the existing update-branch lease before any
# untrusted scene command runs.
BASE_OID="$(fetch_base_oid)"
readonly BASE_OID
checkout_oid="$(git --no-replace-objects rev-parse --verify 'HEAD^{commit}')"
[[ "$checkout_oid" == "$BASE_OID" ]] || \
  fail "checkout HEAD is not the fetched tip of base branch '$BASE_BRANCH'"
read_remote_update_oid
OBSERVED_UPDATE_OID="$REMOTE_UPDATE_OID"
readonly OBSERVED_UPDATE_OID
if [[ -n "$OBSERVED_UPDATE_OID" ]]; then
  git_network fetch --quiet --no-tags --force origin "$UPDATE_REF:$UPDATE_OBSERVED_REF" || \
    fail 'could not fetch the observed update branch'
  fetched_update_oid="$(git --no-replace-objects rev-parse --verify "$UPDATE_OBSERVED_REF^{commit}")"
  [[ "$fetched_update_oid" == "$OBSERVED_UPDATE_OID" ]] || \
    fail 'the update branch moved while it was being inspected; rerun the action'
  remote_email="$(git --no-replace-objects show -s --format=%ae "$OBSERVED_UPDATE_OID")"
  remote_body="$(git --no-replace-objects show -s --format=%B "$OBSERVED_UPDATE_OID")"
  [[ "$remote_email" == "$BOT_EMAIL" && "$remote_body" == *"$COMMIT_MARKER"* ]] || \
    fail "remote branch '$TTYSHOT_UPDATE_BRANCH' is not marked as an action-owned branch; choose a dedicated branch"
fi

# Remove checkout-injected credentials before scene execution. The scene gets
# neither common token environment variable and cannot inherit a GitHub header.
remove_checkout_extraheaders
env -u GH_TOKEN -u GITHUB_TOKEN GIT_NO_REPLACE_OBJECTS=1 \
  "$TTYSHOT_BIN" run "$SCENE_PATH" || \
  fail 'scene generation failed; no commit or push was attempted'

# Restore the exact sanitized local config bytes before invoking Git again. In
# particular, url.*.insteadOf, proxy, transport, pushurl, diff, and hook changes
# made by the scene cannot affect the following inspection or publication.
restored_config="$work_tmp/local-config.restored"
cp -p -- "$SANITIZED_CONFIG_SNAPSHOT" "$restored_config"
mv -f -- "$restored_config" "$LOCAL_CONFIG_PATH"

# Restore and verify the recorded destination. All object inspection ignores
# refs/replace.
reassert_origin
audit_allowed_changes
if ((${#CHANGED_PATHS[@]} == 0)); then
  write_no_change_outputs
  printf 'ttyshot action: screenshots are already current\n'
  exit 0
fi

# Stage each validated path literally. Never treat a generated filename as a
# pathspec and never stage the whole worktree.
for changed_path in "${CHANGED_PATHS[@]}"; do
  env -u GH_TOKEN -u GITHUB_TOKEN GIT_NO_REPLACE_OBJECTS=1 GIT_LITERAL_PATHSPECS=1 \
    git --no-replace-objects -c core.hooksPath="$EMPTY_HOOKS" add -- "$changed_path"
done

# Re-inventory with rename detection and external diff disabled. This catches
# disallowed rename sources and any path introduced during explicit staging.
audit_allowed_changes
STAGED_PATHS=()
while IFS= read -r -d '' staged_path; do
  STAGED_PATHS+=("$staged_path")
done < <(git --no-replace-objects -c diff.renames=false diff --cached --no-ext-diff --no-renames --ignore-submodules=none --name-only -z --)
((${#STAGED_PATHS[@]} > 0)) || fail 'changes disappeared before staging'
for staged_path in "${STAGED_PATHS[@]}"; do
  if ! path_is_allowed "$staged_path"; then
    fail "refusing to commit unexpected staged path: $(quote_path "$staged_path")"
  fi
done

# The scene may have changed config, refs, HEAD, or the remote. Recreate origin,
# refetch the base, and re-read the branch immediately before commit/push.
reassert_origin
fresh_base_oid="$(fetch_base_oid)"
[[ "$fresh_base_oid" == "$BASE_OID" ]] || \
  fail "base branch '$BASE_BRANCH' moved during generation; rerun on the new base"
current_head="$(git --no-replace-objects rev-parse --verify 'HEAD^{commit}')"
[[ "$current_head" == "$BASE_OID" ]] || fail 'checkout HEAD changed during scene generation'
read_remote_update_oid
[[ "$REMOTE_UPDATE_OID" == "$OBSERVED_UPDATE_OID" ]] || \
  fail 'the update branch moved during generation; rerun the action'

staged_tree="$(git --no-replace-objects write-tree)"
commit_oid=''
needs_push=true
if [[ -n "$OBSERVED_UPDATE_OID" ]]; then
  remote_tree="$(git --no-replace-objects show -s --format=%T "$OBSERVED_UPDATE_OID")"
  remote_parent="$(git --no-replace-objects show -s --format=%P "$OBSERVED_UPDATE_OID")"
  remote_subject="$(git --no-replace-objects show -s --format=%s "$OBSERVED_UPDATE_OID")"
  if [[ "$remote_tree" == "$staged_tree" && "$remote_parent" == "$BASE_OID" && \
        "$remote_subject" == "$TTYSHOT_UPDATE_TITLE" ]]; then
    commit_oid="$OBSERVED_UPDATE_OID"
    needs_push=false
    printf 'ttyshot action: the observed update branch already has the generated tree\n'
  fi
fi

if [[ "$needs_push" == true ]]; then
  env -u GH_TOKEN -u GITHUB_TOKEN GIT_NO_REPLACE_OBJECTS=1 \
    git --no-replace-objects \
      -c core.hooksPath="$EMPTY_HOOKS" \
      -c user.name="$BOT_NAME" \
      -c user.email="$BOT_EMAIL" \
      -c commit.gpgsign=false \
      commit --no-verify -m "$TTYSHOT_UPDATE_TITLE" -m "$COMMIT_MARKER"
  commit_oid="$(git --no-replace-objects rev-parse --verify 'HEAD^{commit}')"
  commit_parent="$(git --no-replace-objects show -s --format=%P "$commit_oid")"
  [[ "$commit_parent" == "$BASE_OID" ]] || fail 'generated commit does not have the recorded base as its only parent'
  push_base_oid="$(fetch_base_oid)"
  [[ "$push_base_oid" == "$BASE_OID" ]] || \
    fail "base branch '$BASE_BRANCH' moved before push; rerun on the new base"
  if ! git_network push --force-with-lease="$UPDATE_REF:$OBSERVED_UPDATE_OID" \
      origin "$commit_oid:$UPDATE_REF"; then
    fail 'push failed; the branch may have moved or the workflow needs contents: write permission'
  fi
fi

write_changed_outputs

# Verify the remote still names the exact commit before looking up or creating a
# PR. A later API failure intentionally leaves the dedicated branch in place.
read_remote_update_oid
[[ "$REMOTE_UPDATE_OID" == "$commit_oid" ]] || \
  fail 'the update branch moved after push; refusing to query or create a pull request'

pr_rows="$work_tmp/open-prs.tsv"
if ! gh_network api --method GET "repos/$GITHUB_REPOSITORY/pulls" \
    -f state=open \
    -f head="$REPOSITORY_OWNER:$TTYSHOT_UPDATE_BRANCH" \
    -f base="$BASE_BRANCH" \
    -f per_page=100 \
    --paginate \
    --jq '.[] | [.html_url, .head.ref, .head.sha, .head.repo.full_name, .base.ref, .base.repo.full_name] | @tsv' \
    > "$pr_rows"; then
  fail 'could not query pull requests; grant pull-requests: write permission and verify fork policy'
fi

pr_url=''
exact_pr_count=0
while IFS=$'\t' read -r candidate_url candidate_head candidate_oid candidate_head_repo candidate_base candidate_base_repo; do
  if [[ "$candidate_head" == "$TTYSHOT_UPDATE_BRANCH" && \
        "$candidate_oid" == "$commit_oid" && \
        "$candidate_head_repo" == "$GITHUB_REPOSITORY" && \
        "$candidate_base" == "$BASE_BRANCH" && \
        "$candidate_base_repo" == "$GITHUB_REPOSITORY" ]]; then
    pr_url="$candidate_url"
    ((exact_pr_count += 1))
  fi
done < "$pr_rows"
((exact_pr_count <= 1)) || fail 'GitHub returned multiple open pull requests for the exact repository, head, base, and commit'

if [[ -n "$pr_url" ]]; then
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
  if ! pr_url="$(gh_network pr create \
      --repo "$GITHUB_REPOSITORY" \
      --base "$BASE_BRANCH" \
      --head "$REPOSITORY_OWNER:$TTYSHOT_UPDATE_BRANCH" \
      --title "$TTYSHOT_UPDATE_TITLE" \
      --body-file "$body_file")"; then
    fail 'could not create the pull request; grant pull-requests: write permission and verify fork policy'
  fi
fi

[[ "$pr_url" != *$'\n'* && "$pr_url" != *$'\r'* ]] || fail 'gh returned an invalid pull request URL'
printf 'pr-url=%s\n' "$pr_url" >> "$GITHUB_OUTPUT"
