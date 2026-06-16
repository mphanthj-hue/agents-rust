#!/bin/bash
# run.sh — Launch agents-rust Agent (WSL2 / Linux)
# Usage: ./run.sh [task]
#
# If a task argument is given, runs in non-interactive mode.
# If no argument, opens interactive REPL.
#
# Also works with Ctrl+click / double-click on WSL2 Ubuntu.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$SCRIPT_DIR"

# Custom GCC toolchain
export PATH="/tmp/custom-bin:/tmp/gcc-prefix/usr/bin:/tmp/gcc-prefix/usr/lib/gcc/x86_64-linux-gnu/15:$PATH"
export LIBRARY_PATH="/tmp/gcc-prefix/usr/lib/x86_64-linux-gnu:/tmp/gcc-prefix/lib/x86_64-linux-gnu"
export LD_LIBRARY_PATH="/tmp/gcc-prefix/usr/lib/x86_64-linux-gnu:/tmp/gcc-prefix/lib/x86_64-linux-gnu"
export CC="cc"

# Source cargo
[ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"

BINARY="$PROJECT_DIR/target/debug/agent"

if [ ! -f "$BINARY" ]; then
    echo "Binary not found. Building..."
    cd "$PROJECT_DIR"
    cargo build 2>&1
fi

if [ -n "$1" ]; then
    # Non-interactive: pipe task as input
    echo "$1" | "$BINARY" 2>/dev/null
else
    # Interactive REPL
    exec "$BINARY"
fi
