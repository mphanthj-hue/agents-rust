#!/bin/bash
# install.sh — Cài đặt agents-rust alias cho WSL2
# Chạy: bash install.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ALIAS_CMD="alias agent='$SCRIPT_DIR/run.sh'"
BASHRC="$HOME/.bashrc"

if grep -q "alias agent=" "$BASHRC" 2>/dev/null; then
    sed -i "s|alias agent=.*|$ALIAS_CMD|" "$BASHRC"
    echo "Updated alias in $BASHRC"
else
    echo "" >> "$BASHRC"
    echo "# agents-rust" >> "$BASHRC"
    echo "$ALIAS_CMD" >> "$BASHRC"
    echo "Added alias to $BASHRC"
fi

echo ""
echo "=== Hoàn tất! ==="
echo "Cách dùng:"
echo "  agent                           # Mở REPL (--agent mode)"
echo "  agent 'Liệt kê file trong /tmp' # Chạy 1 task"
echo "  ./mcp.sh                        # MCP server (cho OpenCode)"
echo "  ./agents-rust --dashboard       # Dashboard Web UI"
echo "  ./agents-rust --orchestrate 'task'  # Orchestrator mode"
echo ""
echo "Mở terminal mới để dùng lệnh 'agent'."
