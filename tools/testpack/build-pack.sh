#!/bin/bash
# Builds YAMDV-v2b-test.zip (+ .sha256) from the MSVC-built exes of the CI job "pack exes (MSVC)"
# (artifact `pack-exes`: yamdv.exe, yamdv-dev.exe, xtask.exe, baseline-dev.exe, COMMIT).
# V2-B fix (A-9): exes linked by cargo-xwin/lld-link exited 0xC0000409 on every Windows start, so
# this script no longer builds anything itself and refuses exes without the MSVC Rich header.
# V2-I (prompt 019 §5): PACK=<name> (default YAMDV-v2i-qa) and OWNER_QA.md; uia-client.exe
# goes to target/release/ so tests/driver/uia-tree.yds finds it the way it does on CI.
# Usage: [PACK=name] tools/testpack/build-pack.sh <run-id | pack-exes-dir> [out-dir]
set -euo pipefail
cd "$(dirname "$0")/../.."
src=${1:?CI run id (gh run download ... -n pack-exes) or a directory with the pack exes}
out=${2:-target/pack}
if [[ $src =~ ^[0-9]+$ ]]; then
  dl=$(mktemp -d)
  gh run download "$src" -n pack-exes -D "$dl"
  src=$dl
fi
pack=${PACK:-YAMDV-v2i-qa}
for f in yamdv.exe yamdv-dev.exe xtask.exe baseline-dev.exe uia-client.exe; do
  [[ -f $src/$f ]] || { echo "missing $src/$f" >&2; exit 1; }
  # MSVC link.exe writes a "Rich" header into the DOS stub; lld-link does not.
  head -c 1024 "$src/$f" | grep -qa Rich || { echo "$f was not linked by MSVC link.exe (no Rich header)" >&2; exit 1; }
done
d=$out/$pack
rm -rf "$out" && mkdir -p "$d/baseline" "$d/tests" "$d/target/release"
cp "$src/yamdv.exe" "$d/yamdv.exe"
cp "$src/yamdv-dev.exe" "$d/yamdv-dev.exe"
cp "$src/xtask.exe" "$d/xtask.exe"
cp "$src/baseline-dev.exe" "$d/baseline/yamdv-dev.exe"
cp "$src/uia-client.exe" "$d/target/release/uia-client.exe"
[[ -f $src/COMMIT ]] && echo "exes built by CI from commit $(tr -d '\r\n' < "$src/COMMIT")"
cp tools/testpack/run-tests.cmd tools/testpack/run-tests.ps1 tools/testpack/OWNER_TEST.md tools/testpack/OWNER_QA.md "$d/"
for t in driver fixtures golden corpus; do cp -r "tests/$t" "$d/tests/"; done
find "$d/tests" \( -name '*.log' -o -name '*.tmp.*' \) -delete
# CRLF for the Windows scripts.
for f in "$d/run-tests.cmd" "$d/run-tests.ps1"; do sed -i 's/\r\?$/\r/' "$f"; done
(cd "$out" && rm -f "$pack.zip" && python3 -c 'import shutil,sys; shutil.make_archive(sys.argv[1], "zip", ".", sys.argv[1])' "$pack" &&
  sha256sum "$pack.zip" > "$pack.zip.sha256" && cat "$pack.zip.sha256")
