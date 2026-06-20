#!/bin/bash
# mcp.sh — Launch agents-rust MCP server for OpenCode
# All JSON-RPC output to stdout. Status messages to stderr.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

[ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"

BINARY="$SCRIPT_DIR/target/debug/agents-rust"
if [ ! -f "$BINARY" ]; then
    echo "Building agents-rust..." >&2
    cd "$SCRIPT_DIR" && cargo build --bin agents-rust >&2
fi

exec "$BINARY"
