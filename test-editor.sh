#!/bin/bash
# Integration tests for aimax editor
set -e

AIMAX="./target/release/aimax"
SOCK="$HOME/.aimax/sock"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

pass() { echo -e "${GREEN}PASS${NC}: $1"; }
fail() { echo -e "${RED}FAIL${NC}: $1"; exit 1; }

# Cleanup
cleanup() {
    pkill -f "aimax.*headless" 2>/dev/null || true
    rm -f "$SOCK" /tmp/aimax.log
}
trap cleanup EXIT

# Build
echo "Building..."
cargo build --release 2>&1 | grep -E "error" && fail "Build failed"

# Start headless
cleanup
$AIMAX --headless &
sleep 1

# Test 1: Buffer basics
echo "Test 1: Buffer basics..."
RESULT=$($AIMAX -e '(buffer-name)')
[[ "$RESULT" == *"scratch"* ]] || fail "Expected *scratch*, got: $RESULT"
pass "Default buffer is *scratch*"

# Test 2: Buffer create and switch (IMMEDIATE)
echo "Test 2: Buffer create/switch immediate..."
$AIMAX -e '(buffer-create "test-buf")'
$AIMAX -e '(buffer-switch "test-buf")'
RESULT=$($AIMAX -e '(buffer-name)')
[[ "$RESULT" == '"test-buf"' ]] || fail "Expected test-buf, got: $RESULT"
pass "buffer-create and buffer-switch are immediate"

# Test 3: Find-file minibuffer
echo "Test 3: Find-file minibuffer activation..."
$AIMAX -e '(find-file)'
sleep 0.2
RESULT=$($AIMAX -e '*minibuffer-active*')
[[ "$RESULT" == "#true" ]] || fail "Expected minibuffer active, got: $RESULT"
RESULT=$($AIMAX -e '*minibuffer-prompt*')
[[ "$RESULT" == *"Find file"* ]] || fail "Expected 'Find file' prompt, got: $RESULT"
pass "find-file activates minibuffer"

# Test 4: File completion
echo "Test 4: File completion..."
RESULT=$($AIMAX -e '*minibuffer-matches*')
[[ "$RESULT" != "()" ]] || fail "Expected completions, got empty"
pass "File completions populated"

# Test 5: Cancel minibuffer
$AIMAX -e '(minibuffer-cancel)'
RESULT=$($AIMAX -e '*minibuffer-active*')
[[ "$RESULT" == "#false" ]] || fail "Expected minibuffer inactive after cancel"
pass "minibuffer-cancel works"

# Test 6: Buffer open
echo "Test 6: Open file..."
$AIMAX -e '(buffer-open "README.md")'
sleep 0.3
RESULT=$($AIMAX -e '(buffer-name)')
[[ "$RESULT" == '"README.md"' ]] || fail "Expected README.md, got: $RESULT"
RESULT=$($AIMAX -e '(> (string-length (buffer-text)) 0)')
[[ "$RESULT" == "#true" ]] || fail "Expected non-empty buffer"
pass "buffer-open loads file"

# Test 7: ask-ai creates chat buffer
echo "Test 7: ask-ai chat buffer..."
$AIMAX -e '(ask-ai)'
sleep 0.2
RESULT=$($AIMAX -e '(buffer-name)')
[[ "$RESULT" == '"*chat*"' ]] || fail "Expected *chat*, got: $RESULT"
pass "ask-ai creates and switches to *chat*"

echo ""
echo -e "${GREEN}All tests passed!${NC}"
