#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEST_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/ttyshot-action-tests.XXXXXX")"
trap 'rm -rf "$TEST_ROOT"' EXIT
export PATH="$TEST_ROOT/bin:$PATH"
mkdir -p "$TEST_ROOT/bin"

cat > "$TEST_ROOT/bin/ttyshot" <<'EOF'
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
  spaces)
    mkdir -p 'screen shots'
    printf 'spaced\n' > 'screen shots/output image.png'
    ;;
  outside)
    printf 'updated\n' > screenshots/existing.png
    printf 'unexpected\n' > unexpected.txt
    ;;
  fail)
    printf 'partial\n' > screenshots/partial.png
    exit 42
    ;;
  *) exit 64 ;;
esac
EOF
chmod +x "$TEST_ROOT/bin/ttyshot"

cat > "$TEST_ROOT/bin/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
: "${FAKE_GH_LOG:?}"
printf 'CALL' >> "$FAKE_GH_LOG"
for arg in "$@"; do printf ' <%s>' "$arg" >> "$FAKE_GH_LOG"; done
printf '\n' >> "$FAKE_GH_LOG"
if [[ "$1" == pr && "$2" == list ]]; then
  printf '%s\n' "${FAKE_GH_EXISTING_URL:-}"
elif [[ "$1" == pr && "$2" == create ]]; then
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

assert_no_update_ref() {
  local bare="$1"
  if git --git-dir="$bare" show-ref --verify --quiet refs/heads/ttyshot/update-screenshots; then
    fail 'unexpected update branch was pushed'
  fi
}

new_fixture() {
  local name="$1" mode="$2"
  FIXTURE="$TEST_ROOT/$name"
  BARE="$TEST_ROOT/$name.git"
  mkdir -p "$FIXTURE"
  git init --quiet --bare "$BARE"
  git -C "$FIXTURE" init --quiet -b main
  git -C "$FIXTURE" config user.name 'Fixture Author'
  git -C "$FIXTURE" config user.email fixture@example.test
  mkdir -p "$FIXTURE/screenshots"
  printf '%s\n' "$mode" > "$FIXTURE/scene.toml"
  printf 'original\n' > "$FIXTURE/screenshots/existing.png"
  git -C "$FIXTURE" add scene.toml screenshots/existing.png
  git -C "$FIXTURE" commit --quiet -m 'test: seed fixture'
  git -C "$FIXTURE" remote add origin "$BARE"
  git -C "$FIXTURE" push --quiet -u origin main
  git --git-dir="$BARE" symbolic-ref HEAD refs/heads/main
  OUTPUT="$TEST_ROOT/$name.output"
  GH_LOG="$TEST_ROOT/$name.gh.log"
  : > "$OUTPUT"
  : > "$GH_LOG"
}

run_action() {
  local allowed="$1"
  shift
  (
    cd "$FIXTURE"
    env \
      TTYSHOT_BIN="$TEST_ROOT/bin/ttyshot" \
      TTYSHOT_SCENES=scene.toml \
      TTYSHOT_ALLOWED_PATHS="$allowed" \
      TTYSHOT_UPDATE_BRANCH=ttyshot/update-screenshots \
      TTYSHOT_UPDATE_TITLE='docs: refresh fixture screenshots' \
      TTYSHOT_PR_BODY="${TEST_PR_BODY:-}" \
      TTYSHOT_BASE_BRANCH=main \
      TTYSHOT_DEFAULT_BASE=main \
      GH_TOKEN=test-token \
      GITHUB_OUTPUT="$OUTPUT" \
      RUNNER_TEMP="$TEST_ROOT" \
      FAKE_GH_LOG="$GH_LOG" \
      FAKE_GH_EXISTING_URL="${TEST_EXISTING_URL:-}" \
      bash "$ROOT/scripts/github-action.sh" "$@"
  )
}

printf '1/6 no-change\n'
new_fixture no-change no-change
run_action screenshots
assert_contains "$OUTPUT" 'changed=false'
[[ ! -s "$GH_LOG" ]] || fail 'no-change run called gh'
assert_no_update_ref "$BARE"

printf '2/6 allowed tracked and untracked changes, new PR, body file\n'
new_fixture allowed allowed
TEST_PR_BODY=$'Synthetic fixture update.\nNo private data.'
run_action $'screenshots\nunused path'
unset TEST_PR_BODY
assert_contains "$OUTPUT" 'changed=true'
assert_contains "$OUTPUT" 'pr-url=https://example.test/pull/7'
assert_contains "$GH_LOG" '<--body-file>'
assert_contains "$GH_LOG" 'Synthetic fixture update.'
git --git-dir="$BARE" show refs/heads/ttyshot/update-screenshots:screenshots/existing.png | grep -Fx updated >/dev/null
git --git-dir="$BARE" show refs/heads/ttyshot/update-screenshots:screenshots/new.png | grep -Fx new >/dev/null
changed_names="$(git --git-dir="$BARE" diff-tree --no-commit-id --name-only -r refs/heads/ttyshot/update-screenshots)"
[[ "$changed_names" == $'screenshots/existing.png\nscreenshots/new.png' ]] || fail "unexpected committed paths: $changed_names"

printf '3/6 reject changes outside allowlist\n'
new_fixture outside outside
if run_action screenshots >"$TEST_ROOT/outside.log" 2>&1; then
  fail 'outside-path run succeeded'
fi
assert_contains "$TEST_ROOT/outside.log" 'outside the allowlist'
assert_no_update_ref "$BARE"

printf '4/6 preserve spaces in allowed paths\n'
new_fixture spaces spaces
run_action 'screen shots'
git --git-dir="$BARE" show 'refs/heads/ttyshot/update-screenshots:screen shots/output image.png' | grep -Fx spaced >/dev/null

printf '5/6 scene failure never pushes\n'
new_fixture scene-failure fail
if run_action screenshots >"$TEST_ROOT/scene-failure.log" 2>&1; then
  fail 'failing scene succeeded'
fi
assert_contains "$TEST_ROOT/scene-failure.log" 'no commit or push was attempted'
assert_no_update_ref "$BARE"

printf '6/6 reuse an existing open PR\n'
new_fixture existing-pr allowed
TEST_EXISTING_URL='https://example.test/pull/3'
run_action screenshots
unset TEST_EXISTING_URL
assert_contains "$OUTPUT" 'pr-url=https://example.test/pull/3'
assert_contains "$GH_LOG" '<list>'
if grep -F '<create>' "$GH_LOG" >/dev/null; then
  fail 'existing PR run tried to create another PR'
fi

printf 'PASS: all GitHub Action helper tests passed\n'
