#!/usr/bin/env bash

# Version optional from first arg
VERSION="${1:-latest}"
MDN_SOURCE="https://unpkg.com/@mdn/browser-compat-data@${VERSION}/data.json"

curl -fsSL "${MDN_SOURCE}" | jq -c | tee /dev/stderr | jq -r '
[.svg | .. | objects | select(has("support")) | .support | to_entries[]] as $all
| {
  total_support_entries: ($all | length),
  with_version_removed: [$all[] | select((.value | type == "object" and has("version_removed")) or (.value | type == "array" and any(.[]; type == "object" and has("version_removed"))))] | length,
  with_partial: [$all[] | select((.value | type == "object" and .partial_implementation == true) or (.value | type == "array" and any(.[]; type == "object" and .partial_implementation == true)))] | length,
  with_flags: [$all[] | select((.value | type == "object" and has("flags")) or (.value | type == "array" and any(.[]; type == "object" and has("flags"))))] | length,
  with_notes: [$all[] | select((.value | type == "object" and has("notes")) or (.value | type == "array" and any(.[]; type == "object" and has("notes"))))] | length,
  with_alt_name: [$all[] | select((.value | type == "object" and has("alternative_name")) or (.value | type == "array" and any(.[]; type == "object" and has("alternative_name"))))] | length,
  with_prefix: [$all[] | select((.value | type == "object" and has("prefix")) or (.value | type == "array" and any(.[]; type == "object" and has("prefix"))))] | length,
  with_true_version_added: [$all[] | select((.value | type == "object" and .version_added == true) or (.value | type == "array" and any(.[]; type == "object" and .version_added == true)))] | length,
  with_null_version_added: [$all[] | select((.value | type == "object" and .version_added == null) or (.value | type == "array" and any(.[]; type == "object" and .version_added == null)))] | length,
  with_preview_version_added: [$all[] | select((.value | type == "object" and .version_added == "preview") or (.value | type == "array" and any(.[]; type == "object" and .version_added == "preview")))] | length,
  with_false_version_added: [$all[] | select((.value | type == "object" and .version_added == false) or (.value | type == "array" and any(.[]; type == "object" and .version_added == false)))] | length
}
'
