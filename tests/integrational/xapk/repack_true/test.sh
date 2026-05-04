#!/bin/bash
set -e
APREPO="../../../../target/debug/aprepo"

# Cleanup any lingering server on this port
pkill -f "server.py 8766" 2>/dev/null || true
sleep 0.5

# Start local HTTP server in background
python3 ../../fixtures/server.py 8766 10 &
echo "Server started, running aprepo..."
sleep 1

# Run download
$APREPO --config config.yaml download

# Run process
$APREPO --config config.yaml process

# With repack_xapk: true, output should contain repacked APK(s)
# Since zipalign/apksigner may not be available, accept either APK or XAPK copy
if [ -z "$(ls output/ 2>/dev/null)" ]; then
    echo "FAIL: output directory is empty"
    exit 1
fi

for f in output/*; do
    if ! python3 -c "import zipfile; zipfile.ZipFile('$f').testzip()"; then
        echo "FAIL: $f is not a valid ZIP"
        exit 1
    fi
done

# Cleanup server
pkill -f "server.py 8766" 2>/dev/null || true

echo "PASS: xapk repack_true test"
