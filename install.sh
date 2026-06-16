#!/bin/bash
# install.sh — Cài đặt agents-rust vào PATH và tạo alias cho WSL2
# Chạy: bash install.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ALIAS_CMD="alias agent='$SCRIPT_DIR/run.sh'"
BASHRC="$HOME/.bashrc"

# Add alias if not already present
if grep -q "alias agent=" "$BASHRC" 2>/dev/null; then
    sed -i "s|alias agent=.*|$ALIAS_CMD|" "$BASHRC"
    echo "Updated alias in $BASHRC"
else
    echo "" >> "$BASHRC"
    echo "# agents-rust" >> "$BASHRC"
    echo "$ALIAS_CMD" >> "$BASHRC"
    echo "Added alias to $BASHRC"
fi

# Create desktop entry (WSL2 + Windows)
DESKTOP_DIR="$HOME/Desktop"
mkdir -p "$DESKTOP_DIR"

cat > "$DESKTOP_DIR/agents-rust.desktop" << EOF
[Desktop Entry]
Version=1.0
Type=Application
Name=agents-rust Agent
Comment=Autonomous AI Agent with 15 tools
Exec=$SCRIPT_DIR/run.sh
Icon=utilities-terminal
Terminal=true
Categories=Development;Utility;
EOF
chmod +x "$DESKTOP_DIR/agents-rust.desktop"

# Windows URL shortcut (for WSL2 file explorer)
WIN_USER=$(cmd.exe /c "echo %USERNAME%" 2>/dev/null | tr -d '\r' || echo "")
if [ -n "$WIN_USER" ]; then
    WIN_DESKTOP="/mnt/c/Users/$WIN_USER/Desktop"
    if [ -d "$WIN_DESKTOP" ]; then
        BAT_PATH="$WIN_DESKTOP/agents-rust.bat"
        cat > "$BAT_PATH" << EOFS
@echo off
echo agents-rust Agent
echo Starting...
wsl.exe -d Ubuntu --cd /home/mrken/agents-rust ./run.sh
pause
EOFS
        echo "Created Windows shortcut: $BAT_PATH"
    fi
fi

echo ""
echo "=== Cài đặt hoàn tất! ==="
echo ""
echo "Cách dùng:"
echo "  agent                          # Mở REPL (gõ lệnh)"
echo "  agent 'Liệt kê file trong /tmp' # Chạy 1 task"
echo "  ./run.sh                       # Từ thư mục dự án"
echo ""
echo "Mở terminal mới để dùng lệnh 'agent'."
