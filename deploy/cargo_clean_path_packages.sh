#!/usr/bin/env sh
set -eu

package_names="${TMPDIR:-/tmp}/cargo-path-package-names.$$"
clean_error="${TMPDIR:-/tmp}/cargo-clean-path-package-error.$$"
trap 'rm -f "$package_names" "$clean_error"' EXIT
: > "$package_names"

for repo in "$@"; do
  if [ ! -d "$repo" ]; then
    continue
  fi

  find "$repo" -name Cargo.toml -type f | while IFS= read -r manifest; do
    awk '
      /^\[package\]/ {
        in_package = 1
        next
      }

      /^\[/ {
        if (in_package) {
          exit
        }
        next
      }

      in_package && /^[[:space:]]*name[[:space:]]*=/ {
        name = $0
        sub(/^[^"]*"/, "", name)
        sub(/".*$/, "", name)
        print name
        exit
      }
    ' "$manifest"
  done >> "$package_names"
done

target_dir="${CARGO_TARGET_DIR:-target}"

sort -u "$package_names" | while IFS= read -r package_name; do
  if [ -z "$package_name" ]; then
    continue
  fi

  if cargo clean -p "$package_name" 2> "$clean_error"; then
    echo "removed cooked local package artifact: $package_name" >&2
  elif grep -q "did not match any packages" "$clean_error"; then
    :
  else
    cat "$clean_error" >&2
    exit 1
  fi

  crate_name=$(printf '%s' "$package_name" | tr '-' '_')

  for profile_dir in "$target_dir/release" "$target_dir"/*/release; do
    if [ ! -d "$profile_dir" ]; then
      continue
    fi

    # `cargo clean -p` doesn't reliably remove cooked artifacts for path
    # dependencies that were skeletonized before `cargo chef cook`. Remove the
    # dependency metadata explicitly so the real source copied later is rebuilt.
    rm -rf "$profile_dir/.fingerprint/$crate_name"-*
    rm -f "$profile_dir/deps/$crate_name"-*.d
    rm -f "$profile_dir/deps/lib$crate_name"-*.rlib
    rm -f "$profile_dir/deps/lib$crate_name"-*.rmeta
    rm -f "$profile_dir/deps/lib$crate_name"-*.so
  done
done
