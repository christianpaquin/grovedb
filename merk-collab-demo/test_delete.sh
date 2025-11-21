#!/bin/bash
set -e

echo "=== Testing Delete Operation Fix ==="

# Kill any existing server/auditor
pkill -f "merk-collab" || true
sleep 1

# Start server in background
cd /home/cpaquin/area51/grovedb/merk-collab-demo/server
rm -rf /tmp/merk_collab_server
cargo run > /tmp/server_test.log 2>&1 &
SERVER_PID=$!
echo "Started server (PID: $SERVER_PID)"
sleep 3

# Check if server is running
if ! kill -0 $SERVER_PID 2>/dev/null; then
    echo "ERROR: Server failed to start"
    cat /tmp/server_test.log
    exit 1
fi

# Get the changelog path from server log
CHANGELOG=$(grep "Changelog location:" /tmp/server_test.log | awk '{print $NF}')
echo "Changelog: $CHANGELOG"

# Start client and do operations
echo ""
echo "=== Running Client Operations ==="
cd /home/cpaquin/area51/grovedb/merk-collab-demo/client
npm run client -- insert A 2>&1 | grep -E "Insert|root_hash" || true
sleep 1
npm run client -- insert B 2>&1 | grep -E "Insert|root_hash" || true
sleep 1
npm run client -- insert C 2>&1 | grep -E "Insert|root_hash" || true
sleep 1

echo ""
echo "=== Deleting C ==="
npm run client -- delete 2 2>&1 | grep -E "Delete|root_hash" || true
sleep 2

# Run auditor
echo ""
echo "=== Running Auditor ==="
cd /home/cpaquin/area51/grovedb/merk-collab-demo/auditor
cargo run -- "$CHANGELOG" 2>&1 | tail -20

# Cleanup
kill $SERVER_PID 2>/dev/null || true
echo ""
echo "=== Test Complete ==="
