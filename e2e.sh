#!/bin/bash
# e2e.sh — End-to-end tests for agents-rust
# Usage: bash e2e.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BINARY="$SCRIPT_DIR/target/debug/agents-rust"
PASS=0
FAIL=0

green() { echo -e "\033[32m✓ $1\033[0m"; }
red()   { echo -e "\033[31m✗ $1\033[0m"; }
bail()  { red "$1"; exit 1; }

run_test() {
    local name="$1" desc="$2" cmd="$3" check="$4"
    echo ""
    echo "=== $name ==="
    local out
    out=$(eval "$cmd" 2>&1) || true
    if echo "$out" | eval "$check" 2>/dev/null; then
        green "$desc"; ((PASS++))
    else
        red "$desc"; echo "Output: $(echo "$out" | head -5)"
        ((FAIL++))
    fi
}

# Binary check
echo "=== Binary check ==="
if [ ! -f "$BINARY" ]; then
    cargo build --bin agents-rust 2>&1
fi
[ -f "$BINARY" ] && green "Binary found" || bail "Binary not found"

# 1. --chat mode
run_test "1. --chat mode" "LLM response returned" \
    "timeout 30 '$BINARY' --chat 'Say hello in 3 words' 2>/dev/null" \
    "grep -q '.'"

# 2. --orchestrate mode
run_test "2. --orchestrate mode" "Orchestrator completes" \
    "timeout 90 '$BINARY' --orchestrate 'Count files in current directory' 2>/dev/null" \
    "grep -qiE '(files|thành công|subtask)'"

# 3. MCP initialize + tools/list
run_test "3. MCP initialize" "MCP initialize OK" \
    "echo '{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2024-11-05\",\"capabilities\":{},\"clientInfo\":{\"name\":\"e2e-test\",\"version\":\"0.1.0\"}}}' | timeout 5 '$BINARY' 2>/dev/null" \
    "grep -q '\"result\"'"

run_test "3b. MCP tools/list" "MCP tools/list returns tools" \
    "printf '{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2024-11-05\",\"capabilities\":{},\"clientInfo\":{\"name\":\"e2e-test\",\"version\":\"0.1.0\"}}}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\",\"params\":{}}' | timeout 5 '$BINARY' 2>/dev/null" \
    "grep -q '\"tools\"'"

# 4. --agent mode (check that output has more than just banner)
run_test "4. --agent mode" "Agent processes input" \
    "echo 'exit' | timeout 10 '$BINARY' --agent 2>/dev/null" \
    "grep -qi 'tạm biệt\|goodbye\|bye\|exit'"

# 5. run.sh integration
run_test "5. run.sh" "run.sh works" \
    "timeout 30 bash '$SCRIPT_DIR/run.sh' 'echo hello' 2>/dev/null" \
    "grep -qiE '(hello|hi|chào)'"

# 6. Build zero warnings
echo ""
echo "=== 6. Build zero warnings ==="
BUILD_OUT=$(cargo build --bin agents-rust 2>&1 | grep -E "^(warning|error)" || true)
if [ -z "$BUILD_OUT" ]; then
    green "Build has zero warnings"; ((PASS++))
else
    red "Build has warnings:\n$BUILD_OUT"; ((FAIL++))
fi

# Summary
echo ""
echo "=== Summary ==="
echo -e "\033[32mPassed: $PASS\033[0m"
[ "$FAIL" -gt 0 ] && echo -e "\033[31mFailed: $FAIL\033[0m" || echo -e "\033[32mFailed: $FAIL\033[0m"
echo "Total: $((PASS + FAIL))"

[ "$FAIL" -gt 0 ] && exit 1 || exit 0
