#!/bin/bash
# Records Apple's actool output for two references: the device-family split (Catalog.xcassets
# compiled for iPhone alone and for iPhone plus iPad, tests/ref_samples/target_device_output/) and
# vector rasterisation (Raster.xcassets, one PDF and one SVG set, tests/ref_samples/raster_output/).
# Run on a Mac with Xcode selected (or DEVELOPER_DIR set); compatible with macOS /bin/bash 3.2.
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT=$(pwd)
REF="$ROOT/tests/ref_samples"

if [ "$(uname -s)" != "Darwin" ]; then
    echo "capture-catalog-variants.sh: must run on macOS (Apple's actool records the reference)" >&2
    exit 1
fi
ACTOOL=$(xcrun --find actool)

# compile <catalog> <out dir> <extra actool args...>: Xcode 26's CompileAssetCatalog arguments
# for an iOS app, with the device families given by the caller.
compile() {
    local catalog=$1 out=$2
    shift 2
    rm -rf "$out"
    mkdir -p "$out/compiled"
    local args=(--output-format human-readable-text --notices --warnings
                --output-partial-info-plist "$out/partial.plist"
                --compress-pngs --enable-on-demand-resources YES
                --development-region en "$@"
                --minimum-deployment-target 15.1 --platform iphoneos
                --compile "$out/compiled" "$catalog")
    local status=0
    "$ACTOOL" "${args[@]}" > "$out/actool-output.txt" 2>&1 || status=$?
    plutil -convert xml1 "$out/partial.plist" 2>/dev/null || true
    if [ -f "$out/compiled/Assets.car" ]; then
        xcrun assetutil --info "$out/compiled/Assets.car" > "$out/assets-info.json"
    fi
    {
        echo "Captured $(date -u '+%Y-%m-%dT%H:%M:%SZ') by tools/capture-catalog-variants.sh"
        echo "xcodebuild -version: $(xcodebuild -version | tr '\n' ' ')"
        echo "actool: $ACTOOL"
        echo "exit status: $status"
        echo "command: actool ${args[*]}"
    } | sed "s#$ROOT/##g" > "$out/CAPTURE.txt"
    echo "$out: exit $status"
}

compile "$REF/Catalog.xcassets" "$REF/target_device_output/iphone" --target-device iphone
compile "$REF/Catalog.xcassets" "$REF/target_device_output/iphone-ipad" --target-device iphone --target-device ipad

# Xcode 26.5's assetutil has no --extract, so the car and its --info are the raster reference.
compile "$REF/Raster.xcassets" "$REF/raster_output" --target-device iphone

echo "Captured. Review tests/ref_samples/{target_device_output,raster_output}/*/CAPTURE.txt, then commit tests/ref_samples."
