#!/bin/bash
set -e
APREPO="../../../../target/debug/aprepo"

# Pre-populate cache with fixture
mkdir -p cache/rustore
cp ../../fixtures/test.apk cache/rustore/com.example.app.apk

# Run process (download requires apkeep which may not be available)
$APREPO --config config.yaml process

# Validate output
if [ ! -f "output/com.example.app_1.2.3_universal.apk" ]; then
    echo "FAIL: output/com.example.app_1.2.3_universal.apk not found"
    exit 1
fi

if ! python3 -c "import zipfile; zipfile.ZipFile('output/com.example.app_1.2.3_universal.apk').testzip()"; then
    echo "FAIL: output APK is not a valid ZIP"
    exit 1
fi

echo "PASS: rustore test"
