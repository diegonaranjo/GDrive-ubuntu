#!/usr/bin/env bash
# Battery for tools/secret-scan.sh: every case declares its expected outcome.
# A guard that only ever says "clean" is not a guard, so run this after any
# change to the scanner: tools/guard-battery.sh [repo-dir]
set -uo pipefail

REPO="${1:-$(git rev-parse --show-toplevel)}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/clone/tools/githooks"
cd "$WORK/clone" || exit 1
git init -q -b main .
cp "$REPO/tools/secret-scan.sh" tools/
cp "$REPO/tools/githooks/pre-commit" "$REPO/tools/githooks/pre-push" tools/githooks/
chmod +x tools/secret-scan.sh tools/githooks/*
git config core.hooksPath tools/githooks
git config user.name Battery
echo 'blocked@corp.example' > "$(git rev-parse --absolute-git-dir)/git-guard.deny"
git config user.email someone@users.noreply.github.com
echo seed > seed.txt && git add seed.txt && git commit -qm seed

pass=0; fail=0
run_case() { # run_case <name> <expected_rc> <cmd...>
	local name="$1" want="$2"; shift 2
	local out rc
	out="$("$@" 2>&1)"; rc=$?
	if [[ "$want" == "$rc" ]]; then
		printf '  ok    %-46s rc=%s\n' "$name" "$rc"; pass=$((pass+1))
	else
		printf '  FAIL  %-46s rc=%s esperado %s\n' "$name" "$rc" "$want"
		printf '        %s\n' "$(printf '%s' "$out" | grep -m1 'guard:')"
		fail=$((fail+1))
	fi
	# findings must be reported once, not per duplicated pattern
	local n
	n="$(printf '%s\n' "$out" | grep -c 'exposes a path')"
	if [[ "$name" == *path* && "$n" -gt 1 ]]; then
		printf '  FAIL  %-46s hallazgo duplicado %s veces\n' "$name" "$n"; fail=$((fail+1))
	elif [[ "$name" == *path* ]]; then
		pass=$((pass+1))
	fi
}

echo "== battery =="
printf 'x\n' > a.txt; git add a.txt
git config user.email blocked@corp.example
printf 'x\n' > a.txt; git add a.txt
run_case "deny-listed identity refused" 1 tools/secret-scan.sh staged
git config user.email contributor@external.dev
run_case "contributor identity allowed (note only)" 0 tools/secret-scan.sh staged
git config gitguard.strict-identity true
run_case "contributor refused under strict" 1 tools/secret-scan.sh staged
git config --unset gitguard.strict-identity
git config user.email someone@users.noreply.github.com

git reset -q --hard
printf 'p = /media/%s/vol/project\n' "$(id -un)" > leak.txt; git add leak.txt
run_case "/media/<user> path detected" 1 tools/secret-scan.sh staged
git reset -q --hard
printf 'p = %s/private\n' "$HOME" > leak2.txt; git add leak2.txt
run_case "\$HOME path detected" 1 tools/secret-scan.sh staged
git reset -q --hard
printf 'p = /home/usuario/GoogleDrive\n' > okpath.txt; git add okpath.txt
run_case "documented example path tolerated" 0 tools/secret-scan.sh staged
git reset -q --hard
# Assembled at run time, so this file never holds a credential-shaped literal:
# the repository must stay clean under its own scanner.
tkey="access"; tkey="${tkey}_token"
tval="$(printf 'A%.0s' $(seq 1 24))"
printf 'k = "%s": "%s"\n' "$tkey" "$tval" > tok.txt
git add tok.txt
run_case "token shape detected" 1 tools/secret-scan.sh staged
git reset -q --hard
echo 'X=1' > .env; git add .env
run_case "forbidden file name detected" 1 tools/secret-scan.sh staged
git reset -q --hard; rm -f .env

echo "== real repository =="
run_case "tree of the real repo is clean" 0 "$REPO/tools/secret-scan.sh" tree
run_case "history range is clean" 0 "$REPO/tools/secret-scan.sh" range HEAD

printf '\n%d correctas, %d fallidas\n' "$pass" "$fail"
[[ "$fail" -eq 0 ]]
