#!/bin/bash
# run.sh — Launch agents-rust Agent (WSL2 / Linux)
# Usage: ./run.sh [task]
#
# If a task argument is given, runs in one-shot mode.
# If no argument, opens interactive REPL (--agent mode).

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

[ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"

BINARY="$SCRIPT_DIR/target/debug/agents-rust"

if [ ! -f "$BINARY" ]; then
    echo "Building agents-rust..." >&2
    cd "$SCRIPT_DIR" && cargo build --bin agents-rust >&2
fi

if [ -n "$1" ]; then
    echo "$1" | "$BINARY" --agent 2>/dev/null
else
    exec "$BINARY" --agent
fi
