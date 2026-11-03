#!/usr/bin/env bash
#
# Install the commit/push guards in one clone. Called by setup.sh; safe to re-run.
#
# Everything it configures is repo-local (.git/config and untracked files under
# .git/). The machine's global git config is never modified: that is the user's
# decision, and CONTRIBUTING.md documents the includeIf block for it.
#
# Usage: tools/install-guard.sh [repo-dir]

set -uo pipefail

REPO="${1:-$(git rev-parse --show-toplevel 2>/dev/null || pwd)}"
cd "$REPO" || exit 1

if ! git rev-parse --git-dir >/dev/null 2>&1; then
	echo "guard: $REPO is not a git clone; nothing to install." >&2
	exit 1
fi

GD="$(git rev-parse --absolute-git-dir)"
NOREPLY_RE='@users\.noreply\.github\.com$'

if [ ! -x "$REPO/tools/githooks/pre-commit" ]; then
	echo "guard: tools/githooks is missing; run this from the repository root." >&2
	exit 1
fi
chmod +x "$REPO"/tools/githooks/* "$REPO/tools/secret-scan.sh" \
	"$REPO/tools/guard-battery.sh" 2>/dev/null || true

git config core.hooksPath tools/githooks
echo "guard: core.hooksPath = tools/githooks"

# Keep the policy files out of `git add -A` and out of `git status`.
touch "$GD/info/exclude"
grep -qxF 'git-guard.*' "$GD/info/exclude" || echo 'git-guard.*' >> "$GD/info/exclude"

GLOBAL_EMAIL="$(git config --global user.email || true)"
if [ -z "$GLOBAL_EMAIL" ]; then
	echo "guard: no global identity on this machine; nothing to block."
elif printf '%s' "$GLOBAL_EMAIL" | grep -qE "$NOREPLY_RE"; then
	echo "guard: global identity is already noreply; no deny entry needed."
else
	if [ -f "$GD/git-guard.deny" ] && grep -qxF "$GLOBAL_EMAIL" "$GD/git-guard.deny"; then
		echo "guard: deny entry already present for the global identity."
	else
		printf '%s\n' "$GLOBAL_EMAIL" >> "$GD/git-guard.deny"
		echo "guard: this clone refuses commits signed with '$GLOBAL_EMAIL'."
	fi
fi

# Pin a noreply identity for THIS clone when it would otherwise inherit a
# personal one. A fresh clone used to be the way the leak came back.
if ! printf '%s' "$(git config user.email || true)" | grep -qE "$NOREPLY_RE"; then
	login="$(gh api user -q .login 2>/dev/null || true)"
	[ -n "$login" ] || login="$(git remote get-url origin 2>/dev/null |
		sed -nE 's#.*[/:]([^/:]+)/[^/]+/?$#\1#p')"
	if [ -n "${login:-}" ]; then
		before="$(git config user.email || echo 'unset')"
		git config user.email "$login@users.noreply.github.com"
		echo "guard: this clone signs as $(git config user.email) (it would have used '$before')."
	else
		echo "guard: could not derive a GitHub login; set git config user.email before committing." >&2
	fi
fi

# Enforce noreply signing when this clone is the maintainer's.
if printf '%s' "$(git config user.email || true)" | grep -qE "$NOREPLY_RE"; then
	git config gitguard.strict-identity true
	echo "guard: gitguard.strict-identity = true"
fi

echo "guard: installed."
