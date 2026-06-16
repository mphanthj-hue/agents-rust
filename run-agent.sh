#!/bin/bash
# agents-rust launcher
# Đặt file này ở Desktop, double-click để chạy Agent REPL

PROJECT_DIR="/tmp/opencode/agents-rust"
BINARY="$PROJECT_DIR/target/debug/agent"

# Setup custom toolchain
export PATH="/tmp/custom-bin:/tmp/gcc-prefix/usr/bin:/tmp/gcc-prefix/usr/lib/gcc/x86_64-linux-gnu/15:$PATH"
export LIBRARY_PATH="/tmp/gcc-prefix/usr/lib/x86_64-linux-gnu:/tmp/gcc-prefix/lib/x86_64-linux-gnu"
export LD_LIBRARY_PATH="/tmp/gcc-prefix/usr/lib/x86_64-linux-gnu:/tmp/gcc-prefix/lib/x86_64-linux-gnu"
export CC="cc"

# Source cargo
source "$HOME/.cargo/env" 2>/dev/null

# Run agent
echo "=== agents-rust agent ==="
echo "Project: $PROJECT_DIR"
echo ""
exec "$BINARY"
