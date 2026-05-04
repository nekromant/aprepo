#!/bin/bash
set -e
APREPO="../../../../target/debug/aprepo"
export GITHUB_TOKEN="dummy_token_for_testing"

# Pre-populate cache with fixture
mkdir -p cache/github
cp ../../fixtures/test.apk cache/github/owner_repo.apk

# Run process (download requires gh which may not be available)
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

echo "PASS: github test"
