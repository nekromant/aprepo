#!/bin/bash
set -e
APREPO="../../../../target/debug/aprepo"

# Cleanup any lingering server on this port
pkill -f "server.py 8767" 2>/dev/null || true
sleep 0.5

# Start local HTTP server in background
python3 ../../fixtures/server.py 8767 10 &
echo "Server started, running aprepo..."
sleep 1

# Run download
$APREPO --config config.yaml download

# Run process
$APREPO --config config.yaml process

# With repack_xapk: false, output should contain the original XAPK
if [ ! -f "output/test.xapk" ]; then
    echo "FAIL: output/test.xapk not found"
    exit 1
fi

if ! python3 -c "import zipfile; zipfile.ZipFile('output/test.xapk').testzip()"; then
    echo "FAIL: output/test.xapk is not a valid ZIP"
    exit 1
fi

# Cleanup server
pkill -f "server.py 8767" 2>/dev/null || true

echo "PASS: xapk repack_false test"
