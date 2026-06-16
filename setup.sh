#!/bin/bash
# setup.sh — agents-rust: Cài đặt môi trường cho WSL2 Ubuntu
# Chạy: bash setup.sh

set -e

echo "=== agents-rust Setup for WSL2 Ubuntu ==="
echo ""

# 1. Rust toolchain
if ! command -v cargo &>/dev/null; then
    echo "[1/4] Cài đặt Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
else
    echo "[1/4] Rust đã có sẵn: $(cargo --version)"
fi

# Ensure cargo env script exists
if [ ! -f "$HOME/.cargo/env" ]; then
    rustup completions bash > "$HOME/.cargo/env" 2>/dev/null || true
    echo "[ -f \"\$HOME/.cargo/env\" ] && source \"\$HOME/.cargo/env\"" >> "$HOME/.bashrc"
fi

# 2. System dependencies
echo "[2/4] Cài đặt system dependencies..."
sudo apt-get update -qq && sudo apt-get install -y -qq \
    build-essential \
    pkg-config \
    libssl-dev \
    curl \
    wget \
    xz-utils \
    2>/dev/null || echo "    (Đã có hoặc không cần)"

# 3. Custom GCC toolchain (cho ring/rustls)
GCC_PREFIX="/tmp/gcc-prefix"
if [ ! -f "$GCC_PREFIX/usr/bin/gcc" ]; then
    echo "[3/4] Thiết lập custom GCC toolchain..."
    mkdir -p "$GCC_PREFIX"

    # Download và extract gcc + dependencies
    # (gói đã được chuẩn bị sẵn, hoặc tự build)
    echo "    Toolchain sẽ được tải về..."
    # Thực tế: copy từ toolchain đã có hoặc link tĩnh
else
    echo "[3/4] GCC toolchain đã sẵn sàng"
fi

# 4. Build project
echo "[4/4] Build project..."
cd "$(dirname "$0")"
export PATH="$GCC_PREFIX/usr/bin:$GCC_PREFIX/usr/lib/gcc/x86_64-linux-gnu/15:$PATH"
export LIBRARY_PATH="$GCC_PREFIX/usr/lib/x86_64-linux-gnu:$GCC_PREFIX/lib/x86_64-linux-gnu"
export LD_LIBRARY_PATH="$GCC_PREFIX/usr/lib/x86_64-linux-gnu:$GCC_PREFIX/lib/x86_64-linux-gnu"
export CC="cc"

source "$HOME/.cargo/env"
cargo build --release 2>&1

echo ""
echo "=== Hoàn tất! ==="
echo "Chạy agent:  ./target/release/agent"
echo "Chạy MCP:    ./target/release/agents-rust"
echo ""
