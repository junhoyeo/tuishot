#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEST_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/tuishot-action-tests.XXXXXX")"
trap 'rm -rf "$TEST_ROOT"' EXIT
export PATH="$TEST_ROOT/bin:$PATH"
mkdir -p "$TEST_ROOT/bin"

cat > "$TEST_ROOT/bin/tuishot" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[[ "$1" == run ]]
mode="$(cat "$2")"
case "$mode" in
  no-change) ;;
  allowed)
    printf 'updated\n' > screenshots/existing.png
    printf 'new\n' > screenshots/new.png
    ;;
  credentials)
    [[ -z "${GH_TOKEN+x}" && -z "${GITHUB_TOKEN+x}" ]]
    if git config --get-regexp '^http\..*\.extraheader$' > credential-leak.txt 2>&1; then
      exit 91
    fi
    rm -f credential-leak.txt
    printf 'credential-safe\n' > screenshots/existing.png
    ;;
  spaces)
    mkdir -p 'screen shots'
    printf 'spaced\n' > 'screen shots/output image.png'
    ;;
  literal)
    mkdir -p ':(glob)screen'
    printf 'literal\n' > ':(glob)screen/*.png'
    ;;
  outside)
    printf 'updated\n' > screenshots/existing.png
    printf 'unexpected\n' > unexpected.txt
    ;;
  control)
    printf 'unsafe\n' > $'bad\tname.txt'
    ;;
  rename)
    git mv secret.txt screenshots/renamed.txt
    ;;
  fail)
    printf 'partial\n' > screenshots/partial.png
    exit 42
    ;;
  config-attack)
    original_origin="$(git remote get-url origin)"
    git config --local "url.$FAKE_EVIL_REMOTE.insteadOf" "$original_origin"
    git remote set-url origin "$FAKE_EVIL_REMOTE"
    git config --local remote.origin.pushurl "$FAKE_EVIL_REMOTE"
    git config --local http.https://github.com/.extraheader 'AUTHORIZATION: evil-scene-value'
    mkdir -p .git/evil-hooks
    printf '#!/usr/bin/env bash\nprintf hook-ran > "%s"\n' "$FAKE_HOOK_MARKER" > .git/evil-hooks/post-commit
    chmod +x .git/evil-hooks/post-commit
    git config --local core.hooksPath "$PWD/.git/evil-hooks"
    tree="$(git rev-parse 'HEAD^{tree}')"
    replacement="$(printf 'fake replacement\n' | env \
      GIT_AUTHOR_NAME=Evil GIT_AUTHOR_EMAIL=evil@example.test \
      GIT_COMMITTER_NAME=Evil GIT_COMMITTER_EMAIL=evil@example.test \
      git commit-tree "$tree" -p HEAD)"
    git replace HEAD "$replacement"
    printf 'safe destination\n' > screenshots/existing.png
    ;;
  advance-base)
    git --git-dir="$FAKE_BARE" update-ref refs/heads/main "$FAKE_ADVANCED_OID"
    printf 'updated\n' > screenshots/existing.png
    ;;
  move-update)
    git --git-dir="$FAKE_BARE" update-ref refs/heads/tuishot/update-screenshots "$FAKE_MOVED_OID"
    printf 'updated\n' > screenshots/existing.png
    ;;
  *) exit 64 ;;
esac
EOF
chmod +x "$TEST_ROOT/bin/tuishot"

cat > "$TEST_ROOT/bin/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
: "${FAKE_GH_LOG:?}"
printf 'CALL' >> "$FAKE_GH_LOG"
for arg in "$@"; do printf ' <%q>' "$arg" >> "$FAKE_GH_LOG"; done
printf '\n' >> "$FAKE_GH_LOG"
if [[ "$1" == api ]]; then
  [[ "${FAKE_GH_API_FAIL:-false}" != true ]] || exit 77
  oid="$(git --git-dir="$FAKE_BARE" rev-parse refs/heads/tuishot/update-screenshots)"
  case "${FAKE_GH_MODE:-none}" in
    none) ;;
    exact)
      printf 'https://example.test/pull/3\ttuishot/update-screenshots\t%s\towner/repo\tmain\towner/repo\n' "$oid"
      ;;
    fork)
      printf 'https://example.test/pull/99\ttuishot/update-screenshots\t%s\tforker/repo\tmain\towner/repo\n' "$oid"
      ;;
    fork-and-exact)
      printf 'https://example.test/pull/99\ttuishot/update-screenshots\t%s\tforker/repo\tmain\towner/repo\n' "$oid"
      printf 'https://example.test/pull/3\ttuishot/update-screenshots\t%s\towner/repo\tmain\towner/repo\n' "$oid"
      ;;
    *) exit 78 ;;
  esac
elif [[ "$1" == pr && "$2" == create ]]; then
  [[ "${FAKE_GH_CREATE_FAIL:-false}" != true ]] || exit 79
  body_file=''
  previous=''
  for arg in "$@"; do
    if [[ "$previous" == --body-file ]]; then body_file="$arg"; fi
    previous="$arg"
  done
  [[ -n "$body_file" && -f "$body_file" ]]
  printf '%s\n' 'BODY-BEGIN' >> "$FAKE_GH_LOG"
  cat "$body_file" >> "$FAKE_GH_LOG"
  printf '%s\n' 'BODY-END' >> "$FAKE_GH_LOG"
  printf '%s\n' 'https://example.test/pull/7'
else
  exit 65
fi
EOF
chmod +x "$TEST_ROOT/bin/gh"

fail() {
  printf 'FAIL: %s\n' "$*" >&2
  exit 1
}

assert_contains() {
  local file="$1" expected="$2"
  grep -F -- "$expected" "$file" >/dev/null || fail "$file does not contain: $expected"
}

assert_not_contains() {
  local file="$1" unexpected="$2"
  if grep -F -- "$unexpected" "$file" >/dev/null; then
    fail "$file unexpectedly contains: $unexpected"
  fi
}

assert_no_update_ref() {
  local bare="$1"
  if git --git-dir="$bare" show-ref --verify --quiet refs/heads/tuishot/update-screenshots; then
    fail 'unexpected update branch was pushed'
  fi
}

new_fixture() {
  local name="$1" mode="$2"
  FIXTURE="$TEST_ROOT/$name"
  BARE="$TEST_ROOT/$name.git"
  EVIL_BARE="$TEST_ROOT/$name-evil.git"
  mkdir -p "$FIXTURE"
  git init --quiet --bare "$BARE"
  git init --quiet --bare "$EVIL_BARE"
  git -C "$FIXTURE" init --quiet -b main
  git -C "$FIXTURE" config user.name 'Fixture Author'
  git -C "$FIXTURE" config user.email fixture@example.test
  mkdir -p "$FIXTURE/screenshots"
  printf '%s\n' "$mode" > "$FIXTURE/scene.toml"
  printf 'original\n' > "$FIXTURE/screenshots/existing.png"
  printf 'secret\n' > "$FIXTURE/secret.txt"
  git -C "$FIXTURE" add scene.toml screenshots/existing.png secret.txt
  git -C "$FIXTURE" commit --quiet -m 'test: seed fixture'
  git -C "$FIXTURE" remote add origin "$BARE"
  git -C "$FIXTURE" push --quiet -u origin main
  git --git-dir="$BARE" symbolic-ref HEAD refs/heads/main
  OUTPUT="$TEST_ROOT/$name.output"
  GH_LOG="$TEST_ROOT/$name.gh.log"
  HOOK_MARKER="$TEST_ROOT/$name.hook-ran"
  : > "$OUTPUT"
  : > "$GH_LOG"
  TEST_PR_BODY=''
  TEST_GH_MODE=none
  TEST_GH_API_FAIL=false
  TEST_GH_CREATE_FAIL=false
  TEST_EVENT=workflow_dispatch
  TEST_REPOSITORY=owner/repo
  TEST_ADVANCED_OID=''
  TEST_MOVED_OID=''
}

make_commit_object() {
  local message="$1" author_name="$2" author_email="$3"
  local tree parent oid
  tree="$(git -C "$FIXTURE" rev-parse 'HEAD^{tree}')"
  parent="$(git -C "$FIXTURE" rev-parse HEAD)"
  oid="$(printf '%s\n' "$message" | env \
    GIT_AUTHOR_NAME="$author_name" GIT_AUTHOR_EMAIL="$author_email" \
    GIT_COMMITTER_NAME="$author_name" GIT_COMMITTER_EMAIL="$author_email" \
    git -C "$FIXTURE" commit-tree "$tree" -p "$parent")"
  git -C "$FIXTURE" push --quiet origin "$oid:refs/tuishot-test/objects/${oid:0:12}"
  printf '%s\n' "$oid"
}

make_owned_update() {
  local oid
  oid="$(make_commit_object $'docs: prior update\n\nGenerated-by: tuishot GitHub Action' \
    'github-actions[bot]' '41898282+github-actions[bot]@users.noreply.github.com')"
  git --git-dir="$BARE" update-ref refs/heads/tuishot/update-screenshots "$oid"
  printf '%s\n' "$oid"
}

run_action() {
  local allowed="$1"
  shift
  (
    cd "$FIXTURE"
    env \
      TUISHOT_BIN="$TEST_ROOT/bin/tuishot" \
      TUISHOT_SCENES=scene.toml \
      TUISHOT_ALLOWED_PATHS="$allowed" \
      TUISHOT_UPDATE_BRANCH=tuishot/update-screenshots \
      TUISHOT_UPDATE_TITLE='docs: refresh fixture screenshots' \
      TUISHOT_PR_BODY="$TEST_PR_BODY" \
      TUISHOT_BASE_BRANCH=main \
      TUISHOT_DEFAULT_BASE=main \
      GH_TOKEN=test-token \
      GITHUB_TOKEN=must-not-reach-scene \
      GITHUB_EVENT_NAME="$TEST_EVENT" \
      GITHUB_REPOSITORY="$TEST_REPOSITORY" \
      GITHUB_OUTPUT="$OUTPUT" \
      RUNNER_TEMP="$TEST_ROOT" \
      FAKE_GH_LOG="$GH_LOG" \
      FAKE_GH_MODE="$TEST_GH_MODE" \
      FAKE_GH_API_FAIL="$TEST_GH_API_FAIL" \
      FAKE_GH_CREATE_FAIL="$TEST_GH_CREATE_FAIL" \
      FAKE_BARE="$BARE" \
      FAKE_EVIL_REMOTE="$EVIL_BARE" \
      FAKE_HOOK_MARKER="$HOOK_MARKER" \
      FAKE_ADVANCED_OID="$TEST_ADVANCED_OID" \
      FAKE_MOVED_OID="$TEST_MOVED_OID" \
      bash "$ROOT/scripts/github-action.sh" "$@"
  )
}

case_no=0
case_name() {
  ((case_no += 1))
  printf '%d %s\n' "$case_no" "$*"
}

case_name 'no-change writes false only after success'
new_fixture no-change no-change
run_action screenshots
assert_contains "$OUTPUT" 'changed=false'
assert_contains "$OUTPUT" 'pr-url='
[[ ! -s "$GH_LOG" ]] || fail 'no-change run called gh'
assert_no_update_ref "$BARE"

case_name 'allowed tracked/untracked changes use body file and exact repository scope'
new_fixture allowed allowed
TEST_PR_BODY=$'Synthetic fixture update.\nNo private data.'
run_action $'screenshots\nunused path'
assert_contains "$OUTPUT" 'changed=true'
assert_contains "$OUTPUT" 'pr-url=https://example.test/pull/7'
assert_contains "$GH_LOG" '<--body-file>'
assert_contains "$GH_LOG" '<--repo> <owner/repo>'
assert_contains "$GH_LOG" '<--head> <owner:tuishot/update-screenshots>'
assert_contains "$GH_LOG" 'Synthetic fixture update.'
git --git-dir="$BARE" show refs/heads/tuishot/update-screenshots:screenshots/existing.png | grep -Fx updated >/dev/null
git --git-dir="$BARE" show refs/heads/tuishot/update-screenshots:screenshots/new.png | grep -Fx new >/dev/null
base_oid="$(git --git-dir="$BARE" rev-parse refs/heads/main)"
parent_oid="$(git --git-dir="$BARE" show -s --format=%P refs/heads/tuishot/update-screenshots)"
[[ "$parent_oid" == "$base_oid" ]] || fail 'generated commit parent is not the exact remote base'

case_name 'scene cannot see tokens or checkout extraheaders'
new_fixture credentials credentials
git -C "$FIXTURE" config --local http.https://github.com/.extraheader 'AUTHORIZATION: checkout-secret'
run_action screenshots
[[ ! -e "$FIXTURE/credential-leak.txt" ]] || fail 'scene observed a GitHub extraheader'

case_name 'reject changes outside allowlist'
new_fixture outside outside
if run_action screenshots >"$TEST_ROOT/outside.log" 2>&1; then fail 'outside-path run succeeded'; fi
assert_contains "$TEST_ROOT/outside.log" 'outside the allowlist'
assert_no_update_ref "$BARE"

case_name 'disallowed-to-allowed rename fails even with diff.renames=true'
new_fixture rename rename
git -C "$FIXTURE" config --local diff.renames true
if run_action screenshots >"$TEST_ROOT/rename.log" 2>&1; then fail 'rename run succeeded'; fi
assert_contains "$TEST_ROOT/rename.log" 'secret.txt'
assert_no_update_ref "$BARE"

case_name 'spaces and literal pathspec-looking filenames are staged literally'
new_fixture spaces spaces
run_action 'screen shots'
git --git-dir="$BARE" show 'refs/heads/tuishot/update-screenshots:screen shots/output image.png' | grep -Fx spaced >/dev/null
new_fixture literal literal
run_action ':(glob)screen'
git --git-dir="$BARE" show 'refs/heads/tuishot/update-screenshots::(glob)screen/*.png' | grep -Fx literal >/dev/null

case_name 'control characters in rejected paths are shell-quoted'
new_fixture control control
if run_action screenshots >"$TEST_ROOT/control.log" 2>&1; then fail 'control-path run succeeded'; fi
assert_contains "$TEST_ROOT/control.log" "bad\\tname.txt'"
if LC_ALL=C grep $'\t' "$TEST_ROOT/control.log" >/dev/null; then fail 'raw tab leaked into path diagnostic'; fi

case_name 'scene failure never pushes'
new_fixture scene-failure fail
if run_action screenshots >"$TEST_ROOT/scene-failure.log" 2>&1; then fail 'failing scene succeeded'; fi
assert_contains "$TEST_ROOT/scene-failure.log" 'no commit or push was attempted'
assert_no_update_ref "$BARE"

case_name 'pull request events fail before scene execution'
new_fixture pull-event allowed
TEST_EVENT=pull_request_target
if run_action screenshots >"$TEST_ROOT/pull-event.log" 2>&1; then fail 'pull_request_target run succeeded'; fi
assert_contains "$TEST_ROOT/pull-event.log" 'is not trusted'
grep -Fx original "$FIXTURE/screenshots/existing.png" >/dev/null
assert_no_update_ref "$BARE"

case_name 'wrong checkout fails before scene execution'
new_fixture wrong-checkout allowed
git -C "$FIXTURE" commit --quiet --allow-empty -m 'test: wrong checkout'
if run_action screenshots >"$TEST_ROOT/wrong-checkout.log" 2>&1; then fail 'wrong checkout run succeeded'; fi
assert_contains "$TEST_ROOT/wrong-checkout.log" 'checkout HEAD is not the fetched tip'
grep -Fx original "$FIXTURE/screenshots/existing.png" >/dev/null

case_name 'base advancement during scene is rejected'
new_fixture base-move advance-base
TEST_ADVANCED_OID="$(make_commit_object 'test: advanced base' 'Fixture Author' fixture@example.test)"
if run_action screenshots >"$TEST_ROOT/base-move.log" 2>&1; then fail 'base movement run succeeded'; fi
assert_contains "$TEST_ROOT/base-move.log" 'base branch'
assert_contains "$TEST_ROOT/base-move.log" 'moved during generation'
assert_no_update_ref "$BARE"

case_name 'update branch movement during scene is rejected'
new_fixture update-move move-update
make_owned_update >/dev/null
TEST_MOVED_OID="$(make_commit_object $'docs: moved update\n\nGenerated-by: tuishot GitHub Action' \
  'github-actions[bot]' '41898282+github-actions[bot]@users.noreply.github.com')"
if run_action screenshots >"$TEST_ROOT/update-move.log" 2>&1; then fail 'update movement run succeeded'; fi
assert_contains "$TEST_ROOT/update-move.log" 'update branch moved during generation'

case_name 'replace refs cannot forge remote branch ownership'
new_fixture replace-owner allowed
malicious_oid="$(make_commit_object 'human commit' Human human@example.test)"
git --git-dir="$BARE" update-ref refs/heads/tuishot/update-screenshots "$malicious_oid"
forged_oid="$(make_commit_object $'forged\n\nGenerated-by: tuishot GitHub Action' \
  'github-actions[bot]' '41898282+github-actions[bot]@users.noreply.github.com')"
git -C "$FIXTURE" replace "$malicious_oid" "$forged_oid"
if run_action screenshots >"$TEST_ROOT/replace-owner.log" 2>&1; then fail 'replace-forged ownership succeeded'; fi
assert_contains "$TEST_ROOT/replace-owner.log" 'not marked as an action-owned branch'
[[ "$(git --git-dir="$BARE" rev-parse refs/heads/tuishot/update-screenshots)" == "$malicious_oid" ]] || fail 'malicious branch was overwritten'

case_name 'scene remote/config/replace/hook mutations cannot redirect or intercept publish'
new_fixture config-attack config-attack
run_action screenshots
[[ ! -e "$HOOK_MARKER" ]] || fail 'scene-provided commit hook ran'
git --git-dir="$BARE" show refs/heads/tuishot/update-screenshots:screenshots/existing.png | grep -Fx 'safe destination' >/dev/null
assert_no_update_ref "$EVIL_BARE"

case_name 'exact same-repository PR is reused and same-name fork PR is ignored'
new_fixture existing-pr allowed
TEST_GH_MODE=fork-and-exact
run_action screenshots
assert_contains "$OUTPUT" 'pr-url=https://example.test/pull/3'
assert_not_contains "$GH_LOG" '<create>'
new_fixture fork-only allowed
TEST_GH_MODE=fork
run_action screenshots
assert_contains "$OUTPUT" 'pr-url=https://example.test/pull/7'
assert_contains "$GH_LOG" '<create>'

case_name 'API query failure after push leaves changed=true and empty pr-url'
new_fixture api-failure allowed
TEST_GH_API_FAIL=true
if run_action screenshots >"$TEST_ROOT/api-failure.log" 2>&1; then fail 'API failure run succeeded'; fi
assert_contains "$OUTPUT" 'changed=true'
assert_contains "$OUTPUT" 'pr-url='
if grep -E '^pr-url=.+$' "$OUTPUT" >/dev/null; then fail 'API failure wrote a PR URL'; fi
git --git-dir="$BARE" show-ref --verify --quiet refs/heads/tuishot/update-screenshots || fail 'API failure lost pushed branch'
assert_contains "$TEST_ROOT/api-failure.log" 'fork policy'

case_name 'PR create failure after push leaves changed=true and empty pr-url'
new_fixture create-failure allowed
TEST_GH_CREATE_FAIL=true
if run_action screenshots >"$TEST_ROOT/create-failure.log" 2>&1; then fail 'create failure run succeeded'; fi
assert_contains "$OUTPUT" 'changed=true'
if grep -E '^pr-url=.+$' "$OUTPUT" >/dev/null; then fail 'create failure wrote a PR URL'; fi
git --git-dir="$BARE" show-ref --verify --quiet refs/heads/tuishot/update-screenshots || fail 'create failure lost pushed branch'
assert_contains "$TEST_ROOT/create-failure.log" 'pull-requests: write permission'

case_name 'push permission failure explains contents permission and never calls gh'
new_fixture push-failure allowed
mkdir -p "$BARE/hooks"
printf '#!/usr/bin/env bash\nexit 1\n' > "$BARE/hooks/pre-receive"
chmod +x "$BARE/hooks/pre-receive"
if run_action screenshots >"$TEST_ROOT/push-failure.log" 2>&1; then fail 'rejected push succeeded'; fi
assert_contains "$TEST_ROOT/push-failure.log" 'contents: write permission'
[[ ! -s "$GH_LOG" ]] || fail 'failed push called gh'

case_name 'GITHUB_REPOSITORY is mandatory'
new_fixture missing-repository allowed
TEST_REPOSITORY=''
if run_action screenshots >"$TEST_ROOT/missing-repository.log" 2>&1; then fail 'missing repository run succeeded'; fi
assert_contains "$TEST_ROOT/missing-repository.log" 'GITHUB_REPOSITORY is required'
grep -Fx original "$FIXTURE/screenshots/existing.png" >/dev/null

case_name 'action build boundary ignores consumer Rust wrappers/config'
action_file="$ROOT/action.yml"
assert_contains "$ROOT/scripts/github-action.sh" 'GIT_TERMINAL_PROMPT=0'
assert_contains "$action_file" "cd -- \"\$GITHUB_ACTION_PATH\""
assert_contains "$action_file" 'tuishot-cargo-home.XXXXXX'
assert_contains "$action_file" 'unset RUSTC_WRAPPER RUSTFLAGS CARGO_ENCODED_RUSTFLAGS CARGO_BUILD_RUSTC_WRAPPER'
assert_contains "$action_file" "export CARGO_HOME=\"\$cargo_home\""
assert_contains "$action_file" "--target \"\$target_triple\""
cd_line="$(grep -nF "cd -- \"\$GITHUB_ACTION_PATH\"" "$action_file" | cut -d: -f1)"
build_line="$(grep -nF "cargo build \\" "$action_file" | cut -d: -f1)"
[[ "$cd_line" -lt "$build_line" ]] || fail 'cargo build is not isolated from consumer working directory'

printf 'PASS: %d GitHub Action security scenarios passed\n' "$case_no"
