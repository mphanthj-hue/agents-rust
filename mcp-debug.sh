#!/bin/bash
# Debug wrapper — logs MCP traffic between OpenCode and agents-rust
LOGFILE="/tmp/mcp-debug.log"
echo "=== MCP START $(date) ===" > "$LOGFILE"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
tee -a "$LOGFILE" | "$SCRIPT_DIR/target/debug/agents-rust" 2>/tmp/mcp-stderr.log | tee -a "$LOGFILE"
echo "=== MCP END $(date) ===" >> "$LOGFILE"
