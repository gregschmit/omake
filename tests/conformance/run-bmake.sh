#!/usr/bin/env bash
#
# Run NetBSD's bmake unit-tests against omake and report a pass/fail summary.
#
# The unit-tests harness is driven by the installed `bmake` (which must behave normally); each test
# is executed with the make-under-test via TEST_MAKE=<omake>. Its own `*.exp` golden files are the
# reference. Many failures are expected — omake targets GNU+BSD compatibility, not a bmake clone —
# so this is a conformance signal, not a pass/fail gate.
#
# Usage: run-bmake.sh
#   OMAKE=/path/to/omake  override the make-under-test.
set -uo pipefail

CONF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OMAKE="${OMAKE:-$CONF_DIR/../../target/debug/omake}"

command -v bmake >/dev/null || { echo "error: bmake not found (needed to drive the harness)" >&2; exit 1; }
[ -x "$OMAKE" ] || { echo "error: omake not built at $OMAKE (run: cargo build)" >&2; exit 1; }

"$CONF_DIR/vendor.sh" bmake
UNIT="$CONF_DIR/.netbsd-src/usr.bin/make/unit-tests"

cd "$UNIT"
total=$(grep -cE '^TESTS[[:space:]]*\+=' Makefile)

echo "bmake unit-tests (NetBSD, bmake 20230711) vs omake --compat=bsd:"
out="$(OMAKE_COMPAT=bsd bmake TEST_MAKE="$OMAKE" test 2>&1)"
failed_line="$(printf '%s\n' "$out" | sed -n 's/^Failed tests: //p' | tail -1)"
nfail=$(printf '%s' "$failed_line" | wc -w)

if [ "$nfail" -eq 0 ] && ! printf '%s' "$out" | grep -q '^Failed tests:'; then
  echo "  all requested tests passed"
else
  echo "  $((total - nfail))/$total passed, $nfail failed"
  printf '%s\n' "$failed_line" | tr ' ' '\n' | sed '/^$/d;s/^/  FAIL /'
fi
