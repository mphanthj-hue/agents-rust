#!/bin/bash
# mcp.sh — Launch agents-rust MCP server for opencode
# All JSON-RPC output must go to stdout ONLY.
# Status messages go to stderr to avoid breaking the protocol.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

export PATH="/tmp/custom-bin:/tmp/gcc-prefix/usr/bin:/tmp/gcc-prefix/usr/lib/gcc/x86_64-linux-gnu/15:$PATH"
export LIBRARY_PATH="/tmp/gcc-prefix/usr/lib/x86_64-linux-gnu:/tmp/gcc-prefix/lib/x86_64-linux-gnu"
export LD_LIBRARY_PATH="/tmp/gcc-prefix/usr/lib/x86_64-linux-gnu:/tmp/gcc-prefix/lib/x86_64-linux-gnu"
export CC="cc"

[ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"

BINARY="$SCRIPT_DIR/target/debug/agents-rust"
if [ ! -f "$BINARY" ]; then
    echo "Building agents-rust..." >&2
    cd "$SCRIPT_DIR" && cargo build --bin agents-rust >&2
fi

exec "$BINARY"
