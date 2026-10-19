#!/usr/bin/env bash
# Paint the same displays with this Rust caQtDM and, when CAQTDM_BIN is set,
# with the original C++/Qt caQtDM. Screenshots land in the output directory.
#
#   scripts/compare-displays.sh [output-dir]
#
# CAQTDM_RS   Rust binary (default: target/debug/caqtdm)
# CAQTDM_BIN  original caQtDM binary. Optional.
#             The original is launched under Xvfb. A grab is taken with
#             import, xwd, or -savetoimage, whichever is available.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
out=${1:-"$root/artifacts/compare"}
bin=${CAQTDM_RS:-"$root/target/debug/caqtdm"}
mkdir -p "$out/rust" "$out/upstream" "$out/side-by-side"

if [[ ! -x "$bin" ]]; then
  echo "building $bin" >&2
  (cd "$root" && cargo build -p caqtdm-app)
fi

mapfile -t displays < <(
  cd "$root"
  for candidate in \
    examples/displays/vacuum.ui \
    upstream/caQtDM_Tests/tests.ui \
    upstream/caQtDM_Tests/test.ui \
    upstream/caQtDM_Tests/numeric.ui \
    upstream/caQtDM_Viewer/parserEDM/graphics.edl \
    upstream/caQtDM_Viewer/parserEDM/prcshowcase.prc
  do
    if [[ -f $candidate ]]; then
      printf '%s\n' "$candidate"
    fi
  done
  if [[ -d upstream/caQtDM_Tests ]]; then
    find upstream/caQtDM_Tests -maxdepth 1 -name '*.ui' -print | sort
  fi
)

log="$out/rust-warnings.txt"
: >"$log"
for display in "${displays[@]}"; do
  base=$(basename "$display")
  base=${base%.*}
  echo "rust $display" >&2
  {
    echo "## $display"
    if ! "$bin" snapshot "$root/$display" -o "$out/rust/$base.png"; then
      echo "FAILED $display"
    fi
  } >>"$log" 2>&1
  if [[ ! -s $out/rust/$base.png ]]; then
    echo "no png for $display" >&2
  fi
done

grab_upstream() {
  local display=$1 dest=$2
  if [[ -z ${CAQTDM_BIN:-} || ! -x $CAQTDM_BIN ]]; then
    return 1
  fi
  local shot
  shot=$(mktemp -d)
  # The version splash sits on top of the display. Import the display window
  # itself. A large file such as tests.ui keeps loading includes for a while.
  local picker=$root/scripts/pick-caqtdm-window.py
  if command -v import >/dev/null 2>&1 && command -v xwininfo >/dev/null 2>&1; then
    xvfb-run -a -s "-screen 0 1600x1200x24" bash -c '
      "$1" "$2" >"$3/log.txt" 2>&1 &
      pid=$!
      stem=$(basename "$2")
      stem=${stem%.*}
      id=""
      for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15; do
        sleep 1
        id=$(xwininfo -root -tree 2>/dev/null | python3 "$4" "$stem" || true)
        if [[ -n $id ]]; then
          break
        fi
      done
      if [[ -n $id ]]; then
        import -window "$id" "$3/grab.png" || true
      fi
      kill "$pid" 2>/dev/null || true
      wait "$pid" 2>/dev/null || true
    ' _ "$CAQTDM_BIN" "$display" "$shot" "$picker"
    if [[ -s $shot/grab.png ]]; then
      cp "$shot/grab.png" "$dest"
      rm -rf "$shot"
      return 0
    fi
  fi
  (
    cd "$shot"
    xvfb-run -a timeout 20 "$CAQTDM_BIN" -savetoimage "$display" || true
    found=$(find "$shot" -name '*.png' -print -quit || true)
    if [[ -n $found ]]; then
      cp "$found" "$dest"
    fi
  )
  rm -rf "$shot"
  [[ -s $dest ]]
}

for display in "${displays[@]}"; do
  base=$(basename "$display")
  base=${base%.*}
  dest="$out/upstream/$base.png"
  if grab_upstream "$root/$display" "$dest"; then
    echo "upstream $display" >&2
  fi
done

python3 - "$out" <<'PY'
import sys
from pathlib import Path
out = Path(sys.argv[1])
try:
    from PIL import Image
except ImportError:
    print("pillow is not installed; left the PNGs unmerged")
    raise SystemExit(0)

side = out / "side-by-side"
side.mkdir(exist_ok=True)
for rust in sorted((out / "rust").glob("*.png")):
    other = out / "upstream" / rust.name
    left = Image.open(rust).convert("RGB")
    if not other.is_file():
        continue
    right = Image.open(other).convert("RGB")
    height = max(left.height, right.height)
    canvas = Image.new("RGB", (left.width + right.width + 8, height), (32, 32, 32))
    canvas.paste(left, (0, 0))
    canvas.paste(right, (left.width + 8, 0))
    canvas.save(side / rust.name)
    print(f"side-by-side {rust.name}")
PY

echo "wrote $out"
