# Conformance test suites

Runs omake against the **real** upstream test suites of GNU make and NetBSD bmake, as a coarse
conformance signal beyond the hand-written scenarios in `tests/scenarios/`.

The upstream sources are **vendored on demand** (cloned at a pinned commit into a gitignored
directory) — they are never committed. `vendor.sh` clones only when missing and always checks out the
pinned commit.

| Suite | Source | Pinned commit | Version |
|-------|--------|---------------|---------|
| GNU make | `git.savannah.gnu.org/git/make.git` | `d66a65ad` | 4.4.1 |
| bmake | `github.com/NetBSD/src` (sparse `usr.bin/make`) | `1d2eff37` | 20230711 (netbsd-10) |

Both are pinned to match the `gmake`/`bmake` binaries these tests cross-check against.

## Running

```sh
cargo build                       # build the omake binary under test
tests/conformance/run-bmake.sh    # NetBSD bmake unit-tests
tests/conformance/run-gnu.sh      # GNU make test suite
```

Both honor a `OMAKE=/path/to/omake` override. `run-gnu.sh` also takes optional category arguments
(e.g. `run-gnu.sh features/comments functions/wildcard`); `run-bmake.sh` runs the whole suite.
`vendor.sh [gnu|bmake|all]` vendors without running.

### Prerequisites

- `git`, `perl`, and `bmake` on `PATH`. The GNU runner needs perl's `FindBin`; a minimal shim is
  bundled in `perl-shims/` so no extra perl packages are required.
- Network access for the first vendoring of each suite.

## How to read the results (important)

Both suites do **exact golden-file comparison** (`*.exp` for bmake; per-test expected output for
GNU). They were written to test *those* makes, so they assume that make's exact byte-for-byte output.
omake deliberately has its **own** diagnostic format (`omake: ERROR [file:N] | … ` with a caret,
status lines via a distinct stdout path) rather than gmake's `*** [file:N: target] Error C` or
bmake's `make: …`. So **most "failures" are output-format/wording differences, not semantic bugs** —
omake's makefile *semantics* are covered byte-for-byte by `tests/scenarios/`.

What these suites are genuinely useful for:

- **Robustness** — running omake over ~400 (bmake) + ~1000 (GNU) real makefiles surfaces panics,
  hangs, and parse errors that hand-written scenarios miss.
- **A tracked baseline** — the pass counts should only go up. They climb when omake gains a feature
  the suite exercises, or if omake's diagnostic output is ever aligned to gmake's (the error-format
  item in `TODO.md`).

The GNU runner patches a throwaway copy of the vendored perl driver's preflight (it otherwise aborts
because omake does not claim to be "GNU Make" via `make -v`, and it re-derives `$(MAKE)` via
`$(info …)`). omake is **not** made to impersonate GNU make; the patch lives only in the gitignored
vendored tree.

## Baselines

Captured on Linux against the pinned versions (informational — reproduce with the scripts above):

- **bmake unit-tests:** ~2/396 (the suite is a near-exhaustive test of bmake-specific behavior —
  `.MAKE.*` variables, `:C`/`:@` modifiers, archives, job debugging — most of which omake does not
  replicate).
- **GNU make suite:** see `run-gnu.sh` output (the majority fail on diagnostic-format differences).
