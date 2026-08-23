#!/usr/bin/env sh
set -eu

placeholder='pub fn __cargo_chef_placeholder() {}
fn main() {}'

for repo in "$@"; do
  if [ ! -d "$repo" ]; then
    echo "missing path dependency repo: $repo" >&2
    exit 1
  fi

  find "$repo" -type f \
    ! -name Cargo.toml \
    ! -name Cargo.lock \
    ! -name rust-toolchain \
    ! -name rust-toolchain.toml \
    -delete

  find "$repo" -type d -empty -delete

  find "$repo" -name Cargo.toml -type f | while IFS= read -r manifest; do
    dir="$(dirname "$manifest")"
    if ! grep -q '^\[package\]' "$manifest"; then
      continue
    fi

    mkdir -p "$dir/src"
    printf '%s\n' "$placeholder" > "$dir/src/lib.rs"
    printf '%s\n' 'fn main() {}' > "$dir/src/main.rs"

    sed -n 's/^[[:space:]]*path[[:space:]]*=[[:space:]]*"\([^"]*\.rs\)".*/\1/p' "$manifest" |
      while IFS= read -r target_path; do
        target_file="$dir/$target_path"
        mkdir -p "$(dirname "$target_file")"
        printf '%s\n' "$placeholder" > "$target_file"
      done

    awk '
      function emit() {
        if (kind == "" || name == "" || path != "") {
          return
        }
        print kind " " name
      }

      /^\[\[(bin|test|bench|example)\]\]/ {
        emit()
        kind = $0
        sub(/^\[\[/, "", kind)
        sub(/\]\]$/, "", kind)
        name = ""
        path = ""
        next
      }

      /^\[/ {
        emit()
        kind = ""
        name = ""
        path = ""
        next
      }

      kind != "" && /^[[:space:]]*name[[:space:]]*=/ {
        name = $0
        sub(/^[^"]*"/, "", name)
        sub(/".*$/, "", name)
        next
      }

      kind != "" && /^[[:space:]]*path[[:space:]]*=/ {
        path = $0
        next
      }

      END {
        emit()
      }
    ' "$manifest" | while read -r kind name; do
      case "$kind" in
        bin)
          target_path="src/bin/$name.rs"
          ;;
        test)
          target_path="tests/$name.rs"
          ;;
        bench)
          target_path="benches/$name.rs"
          ;;
        example)
          target_path="examples/$name.rs"
          ;;
        *)
          echo "unsupported cargo target kind: $kind" >&2
          exit 1
          ;;
      esac

      target_file="$dir/$target_path"
      mkdir -p "$(dirname "$target_file")"
      printf '%s\n' "$placeholder" > "$target_file"
    done
  done
done
