#!/usr/bin/env bash

set -u -o pipefail
shopt -s nullglob

files=(samples/*.svg samples/assets/*.svg)

# No samples? Fine. Change this to `exit 1` if samples are mandatory.
((${#files[@]})) || exit 0

fail=0

for f in "${files[@]}"; do
	output="$(run tree-sitter-cli parse --xml "$f" 2>&1)"
	status=$?

	if ((status != 0)); then
		echo "FAIL (non-zero exit): $f"
		printf '%s\n' "$output"
		fail=1
		continue
	fi

	if grep -Eq '<(ERROR|MISSING)([[:space:]>]|/>)' <<<"$output"; then
		echo "FAIL (parse error node): $f"
		printf '%s\n' "$output"
		fail=1
	fi
done

exit "$fail"
