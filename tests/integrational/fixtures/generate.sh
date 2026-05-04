#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FIXTURES_DIR="$SCRIPT_DIR"
mkdir -p "$FIXTURES_DIR"

cd "$FIXTURES_DIR"

generate_apk() {
    local name=$1
    local package=$2
    local version=$3
    local tmpdir=$(mktemp -d)
    cat > "$tmpdir/AndroidManifest.xml" <<EOF
<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="$package"
    android:versionName="$version">
</manifest>
EOF
    python3 -c "
import zipfile
with zipfile.ZipFile('$name', 'w', zipfile.ZIP_DEFLATED) as zf:
    zf.write('$tmpdir/AndroidManifest.xml', 'AndroidManifest.xml')
"
    rm -rf "$tmpdir"
}

generate_xapk() {
    local name=$1
    local package=$2
    local version=$3
    local tmpdir=$(mktemp -d)

    cat > "$tmpdir/AndroidManifest.xml" <<EOF
<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="$package"
    android:versionName="$version">
</manifest>
EOF
    python3 -c "
import zipfile
with zipfile.ZipFile('$tmpdir/base.apk', 'w', zipfile.ZIP_DEFLATED) as zf:
    zf.write('$tmpdir/AndroidManifest.xml', 'AndroidManifest.xml')
"
    for arch in arm64-v8a armeabi-v7a x86_64 x86; do
        python3 -c "
import zipfile
with zipfile.ZipFile('$tmpdir/config.$arch.apk', 'w', zipfile.ZIP_DEFLATED) as zf:
    zf.write('$tmpdir/AndroidManifest.xml', 'AndroidManifest.xml')
"
    done

    cat > "$tmpdir/manifest.json" <<EOF
{
    "package_name": "$package",
    "version_name": "$version",
    "base_apk": "base.apk",
    "split_apks": [
        {"file": "base.apk"},
        {"file": "config.arm64-v8a.apk", "abi": "arm64-v8a"},
        {"file": "config.armeabi-v7a.apk", "abi": "armeabi-v7a"},
        {"file": "config.x86_64.apk", "abi": "x86_64"},
        {"file": "config.x86.apk", "abi": "x86"}
    ],
    "density_splits": []
}
EOF

    python3 -c "
import zipfile
with zipfile.ZipFile('$name', 'w', zipfile.ZIP_DEFLATED) as zf:
    zf.write('$tmpdir/manifest.json', 'manifest.json')
    zf.write('$tmpdir/base.apk', 'base.apk')
    for arch in ['arm64-v8a','armeabi-v7a','x86_64','x86']:
        zf.write(f'$tmpdir/config.{arch}.apk', f'config.{arch}.apk')
"
    rm -rf "$tmpdir"
}

echo "Generating fixtures..."
generate_apk "simple.apk" "com.example.simple" "1.0.0"
generate_xapk "universal.xapk" "com.example.universal" "1.0.0"
echo "Done. Fixtures in $FIXTURES_DIR"
