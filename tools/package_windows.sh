#!/usr/bin/env bash
# Run under MSYS2 UCRT64. Bundle non-system DLLs, including their dependencies.
set -euo pipefail

binary=${1:?usage: package_windows.sh BINARY ARCHIVE}
archive=${2:?usage: package_windows.sh BINARY ARCHIVE}
archive="$(cd "$(dirname "$archive")" && pwd)/$(basename "$archive")"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
cp "$binary" "$stage/bookokrat.exe"

system_dir=$(cygpath -u "${WINDIR:?}/System32")
toolchain_bin=${MINGW_PREFIX:?}/bin
pending=("$stage/bookokrat.exe")
while ((${#pending[@]})); do
  current=${pending[0]}
  pending=("${pending[@]:1}")
  imports=$(objdump -p "$current")
  while IFS= read -r dll; do
    [[ -n "$dll" ]] || continue
    case "$(tr '[:upper:]' '[:lower:]' <<< "$dll")" in
      api-ms-win-*|ext-ms-win-*) continue ;;
    esac
    [[ ! -f "$system_dir/$dll" ]] || continue
    [[ ! -f "$stage/$dll" ]] || continue
    if [[ ! -f "$toolchain_bin/$dll" ]]; then
      echo "Unresolved runtime DLL: $dll (imported by $current)" >&2
      exit 1
    fi
    cp "$toolchain_bin/$dll" "$stage/$dll"
    pending+=("$stage/$dll")
  done < <(awk '/DLL Name:/ {print $3}' <<< "$imports")
done

rm -f "$archive"
(cd "$stage" && 7z a "$archive" ./*)
(cd "$(dirname "$archive")" && sha256sum "$(basename "$archive")") > "$archive.sha256"
