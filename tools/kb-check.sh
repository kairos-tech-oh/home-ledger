#!/usr/bin/env bash
# Run the Check block of each kb claim, one command at a time.
#
#   tools/kb-check.sh                 # every claim
#   tools/kb-check.sh kb/some-claim.md ...
#
# A claim passes when every command in its Check block exits zero. Each
# failing command is printed, so a failure says which promise broke.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 2

files=("$@")
[ ${#files[@]} -eq 0 ] && files=(kb/*.md)
pass=0
fail=0

for f in "${files[@]}"; do
  # The commands inside the ```bash fence under "## Check", with backslash
  # continuations joined and comments dropped.
  mapfile -t cmds < <(awk '
    /^## Check/ {inc = 1; next}
    inc && /^## / {exit}
    inc && /^```bash/ {fence = 1; next}
    inc && fence && /^```/ {fence = 0; next}
    inc && fence {print}
  ' "$f" | sed -e ':a' -e '/\\$/N; s/\\\n//; ta' | grep -v '^\s*$' | grep -v '^\s*#')

  status=PASS
  failed=""
  for c in "${cmds[@]}"; do
    if ! bash -c "$c" >/dev/null 2>&1; then
      status=FAIL
      failed+="    x $c"$'\n'
    fi
  done
  if [ ${#cmds[@]} -eq 0 ]; then
    status=FAIL
    failed="    (no commands in its Check block)"$'\n'
  fi

  printf '%s  %s (%d commands)\n' "$status" "$f" "${#cmds[@]}"
  [ -n "$failed" ] && printf '%s' "$failed"
  if [ "$status" = PASS ]; then pass=$((pass + 1)); else fail=$((fail + 1)); fi
done

echo "== $pass passed, $fail failed"
[ "$fail" -eq 0 ]
