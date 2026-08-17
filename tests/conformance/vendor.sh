#!/usr/bin/env bash
#
# Vendor the GNU make and NetBSD bmake sources at pinned commits, into gitignored directories, for
# the conformance suites (see README.md). Clones only when missing (or when the pinned commit is not
# present); otherwise just re-checks out the pinned commit. The vendored trees are never committed.
#
# Usage: vendor.sh [gnu|bmake|all]   (default: all)
set -euo pipefail

CONF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# GNU Make 4.4.1 (matches the gmake we cross-check against).
GNU_URL="https://git.savannah.gnu.org/git/make.git"
GNU_COMMIT="d66a65ad5a0e31b287f53930b0f09e31801f1613"
GNU_DIR="$CONF_DIR/.gnu-make"

# NetBSD src at the netbsd-10 tip, whose usr.bin/make is bmake 20230711 (matches the installed
# bmake). The tree is huge, so we sparse+partial clone only usr.bin/make.
NETBSD_URL="https://github.com/NetBSD/src.git"
NETBSD_COMMIT="1d2eff37050c088093399be0526404544f166cab"
NETBSD_DIR="$CONF_DIR/.netbsd-src"
NETBSD_SPARSE="usr.bin/make"

has_commit() { git -C "$1" rev-parse --verify --quiet "$2^{commit}" >/dev/null 2>&1; }

# Full shallow clone pinned to a specific commit.
vendor_full() {
  local dir="$1" url="$2" commit="$3"
  if has_commit "$dir" "$commit"; then git -C "$dir" checkout --quiet "$commit"; return; fi
  echo "vendoring $url @ $commit -> ${dir##*/}"
  rm -rf "$dir"; git init --quiet "$dir"
  git -C "$dir" remote add origin "$url"
  git -C "$dir" fetch --quiet --depth 1 origin "$commit"
  git -C "$dir" checkout --quiet FETCH_HEAD
}

# Partial (blob:none) + sparse clone of one subtree of a huge repo, pinned to a commit.
vendor_sparse() {
  local dir="$1" url="$2" commit="$3" sparse="$4"
  if has_commit "$dir" "$commit"; then git -C "$dir" checkout --quiet "$commit"; return; fi
  echo "vendoring $url @ $commit ($sparse) -> ${dir##*/}"
  rm -rf "$dir"; git init --quiet "$dir"
  git -C "$dir" remote add origin "$url"
  git -C "$dir" sparse-checkout init --cone >/dev/null 2>&1
  git -C "$dir" sparse-checkout set "$sparse" >/dev/null 2>&1
  git -C "$dir" fetch --quiet --depth 1 --filter=blob:none origin "$commit"
  git -C "$dir" checkout --quiet "$commit"
}

case "${1:-all}" in
  gnu)   vendor_full   "$GNU_DIR"    "$GNU_URL"    "$GNU_COMMIT" ;;
  bmake) vendor_sparse "$NETBSD_DIR" "$NETBSD_URL" "$NETBSD_COMMIT" "$NETBSD_SPARSE" ;;
  all)
    vendor_full   "$GNU_DIR"    "$GNU_URL"    "$GNU_COMMIT"
    vendor_sparse "$NETBSD_DIR" "$NETBSD_URL" "$NETBSD_COMMIT" "$NETBSD_SPARSE"
    ;;
  *) echo "usage: $0 [gnu|bmake|all]" >&2; exit 2 ;;
esac
