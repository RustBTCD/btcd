#!/usr/bin/env bash
# Checks the schemas against docs/architecture/schemas.md:
# - every example validates against its schema (JSON Schema 2020-12, with jsonschema-cli): RPC, event and hook
#   examples, and the `examples` of any `$defs` entry in a type file;
# - RPC, event and hook files have every member, a name matching the file name, and at least one example;
# - every $ref resolves and stays with its owner: a daemon's own files, rbtcd's files (docs/daemons/main and
#   docs/schemas/bus), and docs/schemas/common.json;
# - objects forbid additional properties, `required` names exist, `oneOf` is only tagged variants or a nullable value,
#   and `anyOf`/`allOf` are not used.
# Needs jq and jsonschema-cli (`cargo install jsonschema-cli`).
set -euo pipefail
cd "$(dirname "$0")/.."

for tool in jq jsonschema-cli; do
    command -v "$tool" >/dev/null || { echo "check-schemas: $tool not found (jsonschema-cli: cargo install jsonschema-cli)" >&2; exit 2; }
done

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
problems=0
fail() { echo "$1: $2" >&2; problems=$((problems + 1)); }

conventions='
  walk(if type == "object" then del(.examples) else . end)
  | paths(type == "object") as $p | getpath($p) as $o | ($p | map(tostring) | join("/")) as $at
  | ( select($o.type == "object" and $o.additionalProperties != false)
      | "/\($at): object without \"additionalProperties\": false" ),
    ( (if ($o.required | type) == "array" then $o.required[] else empty end) as $r
      | select(($o.properties // {}) | has($r) | not)
      | "/\($at): required `\($r)` is not a property" ),
    ( select($o | (has("anyOf") or has("allOf"))) | "/\($at): anyOf and allOf are not allowed" ),
    ( select($o | has("oneOf"))
      | select(((($o.oneOf | length) == 2 and $o.oneOf[1] == {"type": "null"})
                or all($o.oneOf[]; .type == "object" and (.properties.type // {} | has("const")))) | not)
      | "/\($at): oneOf is neither tagged variants nor a nullable value" )'

# Directories a file's $refs may point into, besides docs/schemas/common.json.
owner_dirs() {
    case $1 in
        docs/daemons/main/*) echo "$PWD/docs/daemons/main $PWD/docs/schemas/bus" ;;
        docs/daemons/*) local rest=${1#docs/daemons/}; echo "$PWD/docs/daemons/${rest%%/*}" ;;
        docs/schemas/bus/*) echo "$PWD/docs/schemas/bus" ;;
        *) echo "$PWD/docs/schemas" ;;
    esac
}

# validate FILE POINTER FILTER: validates each element of the jq array FILTER against FILE#POINTER.
validate() {
    local file=$1 pointer=$2 filter=$3 count instances=() i output
    count=$(jq "($filter) | length" "$file")
    [ "$count" -gt 0 ] || return 0
    echo "{\"\$ref\": \"file://$PWD/$file#$pointer\"}" > "$tmp/schema.json"
    for ((i = 0; i < count; i++)); do
        jq "($filter)[$i]" "$file" > "$tmp/example-$i.json"
        instances+=("$tmp/example-$i.json")
    done
    if ! output=$(jsonschema-cli validate -d 2020 --errors-only "$tmp/schema.json" -i "${instances[@]}" 2>&1); then
        fail "$file" "examples of #$pointer: $(echo "$output" | sed "s|$tmp/||g")"
    fi
}

files=$(find docs/schemas docs/daemons -path '*schemas*' -name '*.json' | sort)
for file in $files; do
    if ! jq empty "$file" 2>/dev/null; then fail "$file" "not valid JSON"; continue; fi

    while IFS= read -r problem; do fail "$file" "$problem"; done < <(jq -r "$conventions" "$file")

    # Every $ref points to an existing location inside the file's owner.
    read -r -a dirs <<< "$(owner_dirs "$file")"
    while IFS= read -r ref; do
        path=${ref%%#*}
        fragment=${ref#*#}
        target=$file
        [ -n "$path" ] && target="$(dirname "$file")/$path"
        if [ ! -f "$target" ]; then fail "$file" "\$ref $ref: file not found"; continue; fi
        if [ -n "$fragment" ] && ! jq -e --arg f "$fragment" 'getpath($f | ltrimstr("/") | split("/")) != null' \
            "$target" >/dev/null; then
            fail "$file" "\$ref $ref: #$fragment not found"
        fi
        absolute="$(cd "$(dirname "$target")" && pwd)/$(basename "$target")"
        allowed=false
        [ "$absolute" = "$PWD/docs/schemas/common.json" ] && allowed=true
        for dir in "${dirs[@]}"; do [[ $absolute == "$dir"/* ]] && allowed=true; done
        $allowed || fail "$file" "\$ref $ref refers outside its owner"
    done < <(jq -r '.. | objects | .["$ref"]? | strings' "$file")

    kind=$(jq -r 'if has("rpc") then "rpc" elif has("event") then "event" elif has("hook") then "hook"
                  elif has("$defs") then "types" else "" end' "$file")
    case $kind in
        rpc) members='["rpc", "description", "operator", "request", "response", "examples"]' ;;
        event) members='["event", "description", "payload", "examples"]' ;;
        hook) members='["hook", "description", "emitter", "default", "timeout_secs", "payload", "examples"]' ;;
        types)
            for def in $(jq -r '."$defs" | to_entries[] | select(.value.examples) | .key' "$file"); do
                validate "$file" "/\$defs/$def" ".\"\$defs\".\"$def\".examples"
            done
            continue ;;
        *) fail "$file" "neither an rpc, an event, a hook nor a type file"; continue ;;
    esac

    while IFS= read -r member; do fail "$file" "missing \`$member\`"; done \
        < <(jq -r --argjson m "$members" '$m - keys | .[]' "$file")
    name=$(jq -r --arg k "$kind" '.[$k] // "" | sub("^[^.]*\\."; "")' "$file")
    [ "$name" = "$(basename "$file" .json)" ] || fail "$file" "file name does not match the $kind name"
    [ "$(jq '.examples // [] | length' "$file")" -gt 0 ] || fail "$file" "no examples"

    case $kind in
        rpc) validate "$file" /request '[.examples[].request]'
             validate "$file" /response '[.examples[].response]' ;;
        *) validate "$file" /payload '.examples' ;;
    esac
done

if [ "$problems" -gt 0 ]; then
    echo "check-schemas: $problems problem(s)" >&2
    exit 1
fi
echo "check-schemas: $(echo "$files" | wc -l | tr -d ' ') files, no problems"
