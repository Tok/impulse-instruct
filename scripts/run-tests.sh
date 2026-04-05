#!/usr/bin/env bash
# ─── run-tests.sh ─────────────────────────────────────────────────────────────
# Run tests with optional coverage reporting.
#
# Usage:
#   ./run-tests.sh              # run unit tests only (CI-safe)
#   ./run-tests.sh --coverage   # run tests + generate HTML coverage report
#   ./run-tests.sh --watch      # re-run on file changes (needs cargo-watch)
#   ./run-tests.sh -- sequencer # run only tests matching "sequencer"
#
# For LLM response tests (not CI-safe):
#   ./run-llm-tests.sh
#
# Coverage tool: cargo-tarpaulin (Linux only)
#   Install: cargo install cargo-tarpaulin
#
# Alternative (cross-platform): cargo-llvm-cov
#   Install: cargo install cargo-llvm-cov
#            rustup component add llvm-tools-preview
# ──────────────────────────────────────────────────────────────────────────────
set -euo pipefail
cd "$(dirname "$0")/.."

COVERAGE=false
WATCH=false
FILTER=""
EXTRA=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --coverage)  COVERAGE=true ;;
    --watch)     WATCH=true ;;
    --)          shift; FILTER="${*}"; break ;;
    *)           EXTRA+=("$1") ;;
  esac
  shift
done

# ── Watch mode ────────────────────────────────────────────────────────────────
if $WATCH; then
  if ! command -v cargo-watch &>/dev/null; then
    echo "Installing cargo-watch…"
    cargo install cargo-watch
  fi
  echo "Watching for changes (Ctrl-C to stop)…"
  exec cargo watch -x "test ${FILTER:+-- $FILTER}"
fi

# ── Regular test run ──────────────────────────────────────────────────────────
echo "══════════════════════════════════════════"
echo "  Running tests…"
echo "══════════════════════════════════════════"
set +e
cargo test ${FILTER:+-- $FILTER} "${EXTRA[@]}" 2>&1 | grep -Ev "^running 0 tests$|^test result: ok\. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered"
TEST_EXIT=${PIPESTATUS[0]}
set -e
if [ "$TEST_EXIT" -ne 0 ]; then exit "$TEST_EXIT"; fi

# ── Coverage ──────────────────────────────────────────────────────────────────
if $COVERAGE; then
  echo ""
  echo "══════════════════════════════════════════"
  echo "  Generating coverage report…"
  echo "══════════════════════════════════════════"

  REPORT_DIR="target/coverage"
  mkdir -p "$REPORT_DIR"

  # Try tarpaulin first (Linux, simpler)
  if command -v cargo-tarpaulin &>/dev/null; then
    echo "Using cargo-tarpaulin…"
    cargo tarpaulin \
      --out Html \
      --output-dir "$REPORT_DIR" \
      --exclude-files "src/main.rs" \
      --timeout 120 \
      ${FILTER:+-- $FILTER}

    echo ""
    echo "✓ HTML report: ${REPORT_DIR}/tarpaulin-report.html"
    if command -v xdg-open &>/dev/null; then
      xdg-open "${REPORT_DIR}/tarpaulin-report.html" &>/dev/null &
    fi

  # Try llvm-cov (cross-platform)
  elif command -v cargo-llvm-cov &>/dev/null; then
    echo "Using cargo-llvm-cov…"
    cargo llvm-cov \
      --html \
      --output-dir "$REPORT_DIR" \
      ${FILTER:+-- $FILTER}

    echo ""
    echo "✓ HTML report: ${REPORT_DIR}/index.html"
    if command -v xdg-open &>/dev/null; then
      xdg-open "${REPORT_DIR}/index.html" &>/dev/null &
    elif command -v open &>/dev/null; then
      open "${REPORT_DIR}/index.html"
    fi

  else
    echo "No coverage tool found. Install one:"
    echo ""
    echo "  Option 1 — tarpaulin (Linux, easiest):"
    echo "    cargo install cargo-tarpaulin"
    echo ""
    echo "  Option 2 — llvm-cov (cross-platform):"
    echo "    cargo install cargo-llvm-cov"
    echo "    rustup component add llvm-tools-preview"
    echo ""
    echo "Tests ran successfully (no coverage report generated)."
  fi
fi
