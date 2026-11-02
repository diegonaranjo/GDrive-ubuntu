#!/usr/bin/env bash
#
# Identity and secret guard.
#
# Why this exists: a commit pushed to GitHub is effectively permanent.
# Rewriting refs does not remove it — the objects keep answering over the API
# and the tarball endpoint until the repository itself is deleted, which is the
# only "undo" and costs the whole history. So nothing personal may reach a push.
#
# The guard stores no personal identifiers on purpose. Allowed identities are
# defined by shape (GitHub noreply addresses) plus an optional untracked
# override file; the path rules are derived at run time from this machine's own
# user and home directory. A leak-free tool cannot itself become the leak.
#
# Usage:
#   tools/secret-scan.sh staged          # pre-commit: identity + staged diff
#   tools/secret-scan.sh range <A..B>    # pre-push and CI
#   tools/secret-scan.sh tree            # audit every tracked file
#
# Extra allowed e-mails (for a machine whose identity is intentional):
#   echo 'someone@trusted.example' >> "$(git rev-parse --git-dir)/git-guard.local"
#
# Identity policy, deliberately asymmetric so it never blocks a contributor:
#   - $(git-dir)/git-guard.deny   untracked list of e-mails/addresses that must
#                                 never sign a commit here (setup.sh seeds it
#                                 from this machine's global identity).
#   - git config gitguard.strict-identity true   require a noreply address for
#                                 every commit (setup.sh enables it only when
#                                 this clone already has a noreply identity,
#                                 i.e. it is the maintainer's).

set -uo pipefail

ALLOWED_EMAIL_RE='^[A-Za-z0-9._+-]+@users\.noreply\.github\.com$'
GITDIR="$(git rev-parse --git-dir 2>/dev/null || echo .git)"
LOCAL_ALLOW="$GITDIR/git-guard.local"
LOCAL_DENY="$GITDIR/git-guard.deny"
STRICT="$(git config --bool gitguard.strict-identity 2>/dev/null || echo false)"

CONTENT_PATTERNS=(
	'BEGIN [A-Z ]*PRIVATE KEY'
	'\bghp_[A-Za-z0-9]{6,}'
	'\bgithub_pat_[A-Za-z0-9_]{10,}'
	'\bAKIA[0-9A-Z]{16}'
	'\bxox[abprs]-[A-Za-z0-9-]{10,}'
	'access_token["'"'"' :]+[A-Za-z0-9._/-]{15,}'
	'refresh_token["'"'"' :]+[A-Za-z0-9._/-]{15,}'
	'client_secret["'"'"' :]+[A-Za-z0-9._/-]{10,}'
	'Authorization: *(Bearer|Basic) +[A-Za-z0-9/+=._-]{10,}'
	'://[^/@[:space:]]{3,}:[^/@[:space:]]{3,}@'
)

FORBIDDEN_NAMES=(
	'.env' '.env.*' '*.pem' '*.key' '*.p12' '*.pfx' 'id_rsa*' 'id_ed25519*'
	'rclone.conf' 'credentials' 'credentials.*' '*_credentials*' '.netrc'
	'*.kdb' '*.kdbx' 'settings.local.json'
)

# Paths that belong to whoever runs this script, so the rule travels with the
# machine rather than being written down in the repository.
PERSONAL_PATHS=()
add_personal_path() {
	local p="${1:-}"
	[[ -n "$p" ]] || return 0
	case " ${PERSONAL_PATHS[*]-} " in
	*" $p "*) return 0 ;;
	esac
	PERSONAL_PATHS+=("$p")
}
add_personal_path "${HOME:-}"
for u in "${USER:-}" "$(id -un 2>/dev/null || true)"; do
	[[ -n "$u" ]] || continue
	add_personal_path "/home/$u"
	add_personal_path "/media/$u"
	add_personal_path "/mnt/$u"
	add_personal_path "/Users/$u"
done

fail=0
say() {
	printf 'guard: %s\n' "$*"
	fail=1
}

email_denied() {
	local e="${1:-}" line
	[[ -n "$e" ]] || return 1
	[[ -f "$LOCAL_DENY" ]] || return 1
	while IFS= read -r line; do
		[[ -n "$line" ]] || continue
		if [[ "$e" == "$line" || "$e" == *"$line" ]]; then
			return 0
		fi
	done < "$LOCAL_DENY"
	return 1
}

email_allowed() {
	local e="${1:-}"
	[[ -n "$e" ]] || return 0
	[[ "$e" =~ $ALLOWED_EMAIL_RE ]] && return 0
	[[ -f "$LOCAL_ALLOW" ]] && grep -qxF "$e" "$LOCAL_ALLOW" && return 0
	return 1
}

check_identity() {
	local who="$1" e="${2:-}"
	[[ -n "$e" ]] || return 0
	if email_denied "$e"; then
		say "$who signed with an address this repository blocks ($e)"
		echo "       listed in $LOCAL_DENY (untracked, machine-local policy)."
		echo "       fix: git config user.email \"<you>@users.noreply.github.com\""
		return 0
	fi
	if [[ "$STRICT" == "true" ]] && ! email_allowed "$e"; then
		say "$who identity is not allowed here: <$e>"
		echo "       this clone enforces noreply signing (gitguard.strict-identity)."
		echo "       fix: git config user.email \"<you>@users.noreply.github.com\""
		echo "       or add it to $LOCAL_ALLOW if it is intentional."
		return 0
	fi
	if ! email_allowed "$e"; then
		printf 'guard: note: %s signs with <%s>, not a noreply address\n' "$who" "$e"
		echo "       allowed for contributors; the maintainer's clone sets"
		echo "       gitguard.strict-identity=true to make this a refusal."
	fi
	return 0
}

check_names() {
	local f base pat
	for f in "$@"; do
		[[ -n "$f" ]] || continue
		base="$(basename -- "$f")"
		for pat in "${FORBIDDEN_NAMES[@]}"; do
			# shellcheck disable=SC2053
			[[ "$base" == $pat ]] && say "forbidden file name would be published: $f"
		done
	done
}

# scan_stream <label> reads the text to inspect from stdin.
scan_stream() {
	local label="$1" pat hit
	if [[ -t 0 ]]; then return 0; fi
	local buf
	buf="$(cat)"
	[[ -n "$buf" ]] || return 0
	for pat in "${CONTENT_PATTERNS[@]}"; do
		while IFS= read -r hit; do
			say "$label matches a secret shape /$pat/: ${hit:0:180}"
		done < <(printf '%s\n' "$buf" | grep -IiE -- "$pat" | cut -c1-300 || true)
	done
	for pat in "${PERSONAL_PATHS[@]}"; do
		while IFS= read -r hit; do
			say "$label exposes a path belonging to this machine ($pat): ${hit:0:180}"
		done < <(printf '%s\n' "$buf" | grep -IE -- "$pat" | cut -c1-300 || true)
	done
}

# scan_file <path> greps the file itself, so binaries are skipped by grep -I.
scan_file() {
	local f="$1" pat hit
	for pat in "${CONTENT_PATTERNS[@]}"; do
		while IFS= read -r hit; do
			say "$f matches a secret shape /$pat/: ${hit:0:180}"
		done < <(grep -IhiE -e "$pat" -- "$f" | cut -c1-300 || true)
	done
	for pat in "${PERSONAL_PATHS[@]}"; do
		while IFS= read -r hit; do
			say "$f exposes a path belonging to this machine ($pat): ${hit:0:180}"
		done < <(grep -Ih -e "$pat" -- "$f" | cut -c1-300 || true)
	done
}

mode="${1:-}"
case "$mode" in
staged)
	check_identity "configured author" "$(git config user.email || true)"
	names="$(git diff --cached --name-only || true)"
	[[ -n "$names" ]] && check_names $names
	scan_stream "staged content" <<< "$(git diff --cached -U0)"
	;;
range)
	shift
	revs="${1:-}"
	if [[ -z "$revs" ]]; then
		say "range mode needs a revision range (A..B) or a single revision"
		exit 1
	fi
	if [[ "$revs" == *..* ]]; then
		while IFS=$'\t' read -r ae ce; do
			check_identity "commit author" "$ae"
			check_identity "commit committer" "$ce"
		done < <(git log --format='%ae%x09%ce' "$revs" 2>/dev/null || true)
		# shellcheck disable=SC2046
		check_names $(git diff --name-only "$revs" 2>/dev/null || true)
		scan_stream "pushed diff" <<< "$(git diff -U0 "$revs" 2>/dev/null)"
	else
		while IFS=$'\t' read -r ae ce; do
			check_identity "commit author" "$ae"
			check_identity "commit committer" "$ce"
		done < <(git log --format='%ae%x09%ce' -1 "$revs" 2>/dev/null || true)
		# shellcheck disable=SC2046
		check_names $(git show --name-only --format= "$revs" 2>/dev/null || true)
		# `git show` also covers a root commit, which has no parent to diff.
		scan_stream "pushed commit" <<< "$(git show -U0 --format= "$revs" 2>/dev/null)"
	fi
	;;
tree)
	# shellcheck disable=SC2046
	check_names $(git ls-files || true)
	while IFS= read -r f; do
		[[ -f "$f" ]] && scan_file "$f"
	done < <(git ls-files || true)
	;;
*)
	echo 'usage: tools/secret-scan.sh staged | range <A..B> | tree' >&2
	exit 2
	;;
esac

if ((fail)); then
	cat >&2 <<'ADVICE'

Refusing to continue. Fix before pushing:
  - wrong identity : git config user.email "<you>@users.noreply.github.com"
                     (per clone; add an includeIf block in ~/.gitconfig so new
                     clones get it without asking again)
  - secret content : remove it, and rotate whatever was already exposed
  - personal path  : use ~ or the documented example path /home/usuario
`git push --no-verify` skips the hook on purpose only: what is pushed stays.
ADVICE
	exit 1
fi
echo "guard: clean (identity, secret shapes, machine paths, file names)"
