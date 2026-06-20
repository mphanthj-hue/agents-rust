#!/bin/bash
# Debug wrapper to see what opencode sends to the MCP server
LOGFILE="/tmp/mcp-debug.log"
echo "=== MCP START $(date) ===" > "$LOGFILE"
# Read what opencode sends, log it, and pipe to the real binary
tee -a "$LOGFILE" | /home/mrken/agents-rust/target/debug/agents-rust 2>/tmp/mcp-stderr.log | tee -a "$LOGFILE"
echo "=== MCP END $(date) ===" >> "$LOGFILE"
