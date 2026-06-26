#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
GRAMMARS_DIR="${ROOT}/grammars"
ZED_LANGUAGES_DIR="${ROOT}/editors/zed-svg/languages"

if [[ ! -d "${GRAMMARS_DIR}" ]]; then
	printf 'missing grammars dir: %s\n' "${GRAMMARS_DIR}" >&2
	exit 1
fi

if [[ ! -d "${ZED_LANGUAGES_DIR}" ]]; then
	printf 'missing Zed languages dir: %s\n' "${ZED_LANGUAGES_DIR}" >&2
	exit 1
fi

TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

MAP_FILE="${TMP_DIR}/grammar-names.tsv"
: >"${MAP_FILE}"

toml_string() {
	local key=$1
	local file=$2

	awk -v key="${key}" '
		$0 ~ "^[[:space:]]*" key "[[:space:]]*=" {
			value = $0
			sub(/^[^=]*=[[:space:]]*/, "", value)
			if (match(value, /^"([^"\\]|\\.)*"/)) {
				value = substr(value, RSTART + 1, RLENGTH - 2)
				gsub(/\\"/, "\"", value)
				print value
				exit
			}
		}
	' "${file}"
}

for config in "${ZED_LANGUAGES_DIR}"/*/config.toml; do
	[[ -f "${config}" ]] || continue
	grammar=$(toml_string grammar "${config}")
	name=$(toml_string name "${config}")

	if [[ -z "${grammar}" || -z "${name}" ]]; then
		printf 'missing name or grammar in %s\n' "${config}" >&2
		exit 1
	fi

	printf '%s\t%s\n' "${grammar}" "${name}" >>"${MAP_FILE}"
done

rewrite_language_names() {
	local src=$1
	local dest=$2

	awk -v map_file="${MAP_FILE}" '
		BEGIN {
			while ((getline line < map_file) > 0) {
				split(line, parts, "\t")
				if (parts[1] != "" && parts[2] != "") {
					names[parts[1]] = parts[2]
				}
			}
			close(map_file)
		}

		{
			line = $0
			for (grammar in names) {
				line = replace_literal(line, "(#set! injection.language \"" grammar "\")", "(#set! injection.language \"" names[grammar] "\")")
				line = replace_literal(line, "(#set! language \"" grammar "\")", "(#set! language \"" names[grammar] "\")")
			}
			print line
		}

		function replace_literal(value, old, replacement, output, index_at) {
			output = ""
			while ((index_at = index(value, old)) > 0) {
				output = output substr(value, 1, index_at - 1) replacement
				value = substr(value, index_at + length(old))
			}
			return output value
		}
	' "${src}" >"${dest}"
}

patch_svg_locals_for_zed() {
	local file=$1
	local patched=${TMP_DIR}/locals.scm

	awk '
		$0 == " (#match? @local.reference \"^#\"))" {
			print " (#match? @local.reference \"^#\")"
			print " (#strip! @local.reference \"^#\"))"
			next
		}
		{ print }
		$0 == "    (id_token) @local.definition)))" {
			print ""
			print "; Zed-side divergence: strip leading `#` from references so they match bare"
			print "; `id` definitions. Upstream keeps the raw token because Helix rejects `#strip!`."
		}
	' "${file}" >"${patched}"
	mv "${patched}" "${file}"
}

while IFS=$'\t' read -r grammar _name; do
	grammar_dir_name=${grammar//_/-}
	src_dir="${GRAMMARS_DIR}/tree-sitter-${grammar_dir_name}/queries"
	dest_dir="${ZED_LANGUAGES_DIR}/${grammar_dir_name}"

	if [[ ! -d "${src_dir}" ]]; then
		printf 'missing grammar queries dir for %s: %s\n' "${grammar}" "${src_dir}" >&2
		exit 1
	fi

	if [[ ! -d "${dest_dir}" ]]; then
		printf 'missing Zed language dir for %s: %s\n' "${grammar}" "${dest_dir}" >&2
		exit 1
	fi

	for src in "${src_dir}"/*.scm; do
		[[ -f "${src}" ]] || continue
		dest="${dest_dir}/$(basename -- "${src}")"
		rewrite_language_names "${src}" "${dest}"

		case "${grammar}:$(basename -- "${src}")" in
			svg:locals.scm) patch_svg_locals_for_zed "${dest}" ;;
			*) ;;
		esac
	done
done <"${MAP_FILE}"
