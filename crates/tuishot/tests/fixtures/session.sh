#!/bin/sh
set -eu
mode=${1:?mode}
case "$mode" in
  basic)
    printf 'READY\n'
    IFS= read -r input
    printf 'GOT:%s\n' "$input"
    IFS= read -r _hold
    ;;
  two)
    printf '\033[2J\033[HREADY FIRST\n'
    IFS= read -r input
    printf '\033[2J\033[HSECOND:%s\n' "$input"
    IFS= read -r _hold
    ;;
  env)
    printf 'CWD=%s\nHOME=%s\nLEAK=%s\n' "$PWD" "${HOME-unset}" "${TUISHOT_TEST_LEAK-unset}"
    IFS= read -r _hold
    ;;
  resize)
    printf 'READY\n'
    IFS= read -r _input
    set -- $(stty size)
    printf 'SIZE=%sx%s\n' "$2" "$1"
    IFS= read -r _hold
    ;;
  timeout)
    printf 'LAST FRAME\n'
    IFS= read -r _hold
    ;;
  *) exit 2 ;;
esac
