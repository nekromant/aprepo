#!/bin/bash
set -e
APREPO="../../../../target/debug/aprepo"

# Clean previous state
rm -rf cache output state.yaml

# Kill any lingering server on port 8765
pkill -f "server.py 8765" 2>/dev/null || true
sleep 0.5

# Start local HTTP server in background (longer duration for two runs)
python3 ../../fixtures/server.py 8765 20 &
echo "Server started, running aprepo..."
sleep 1

# First download — should be fresh
output1=$($APREPO --config config.yaml download 2>&1)
echo "$output1"
if ! echo "$output1" | grep -q "Fresh download"; then
    echo "FAIL: first download did not print 'Fresh download' message"
    exit 1
fi

# Second download — should be cache hit (metadata unchanged)
output2=$($APREPO --config config.yaml download 2>&1)
echo "$output2"
if ! echo "$output2" | grep -q "Cache hit"; then
    echo "FAIL: second download did not print 'Cache hit' message"
    exit 1
fi

# Run process
$APREPO --config config.yaml process

# Validate output
OUTPUT_APK="output/com.example.app_1.2.3_universal.apk"
if [ ! -f "$OUTPUT_APK" ]; then
    echo "FAIL: $OUTPUT_APK not found"
    ls output/
    exit 1
fi

if ! python3 -c "import zipfile; zipfile.ZipFile('$OUTPUT_APK').testzip()"; then
    echo "FAIL: $OUTPUT_APK is not a valid ZIP"
    exit 1
fi

# Cleanup server
pkill -f "server.py 8765" 2>/dev/null || true

echo "PASS: webdl test"
