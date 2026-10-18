#!/bin/bash
# Records Apple's actool output for an iOS app icon in Xcode's single-size form
# (tests/ref_samples/IosSingleIcon.xcassets), the reference the single-size icon path is tested against.
# Run on a Mac with Xcode selected, from anywhere; writes tests/ref_samples/ios_single_output/<case>/.
# Compatible with macOS /bin/bash 3.2.
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT=$(pwd)
CATALOG="$ROOT/tests/ref_samples/IosSingleIcon.xcassets"
OUT_ROOT="$ROOT/tests/ref_samples/ios_single_output"

if [ "$(uname -s)" != "Darwin" ]; then
    echo "capture-ios-single-icon.sh: must run on macOS (Apple's actool records the reference)" >&2
    exit 1
fi
ACTOOL=$(xcrun --find actool)

# case name, platform, minimum deployment target. An older target and a new Xcode 26 app's default,
# because what actool emits for an icon can depend on the target.
CASES="iphoneos-15.1 iphoneos 15.1
iphoneos-26.0 iphoneos 26.0
iphonesimulator-15.1 iphonesimulator 15.1"

echo "$CASES" | while read -r name platform target; do
    out="$OUT_ROOT/$name"
    rm -rf "$out"
    mkdir -p "$out/compiled"
    # The arguments Xcode 26's CompileAssetCatalog step passes for an iPhone and iPad app.
    args=(--output-format human-readable-text --notices --warnings
          --output-partial-info-plist "$out/partial.plist"
          --app-icon AppIcon --compress-pngs --enable-on-demand-resources YES
          --development-region en --target-device iphone --target-device ipad
          --minimum-deployment-target "$target" --platform "$platform"
          --compile "$out/compiled" "$CATALOG")
    status=0
    "$ACTOOL" "${args[@]}" > "$out/actool-output.txt" 2>&1 || status=$?
    plutil -convert xml1 "$out/partial.plist" 2>/dev/null || true
    if [ -f "$out/compiled/Assets.car" ]; then
        xcrun assetutil --info "$out/compiled/Assets.car" > "$out/assets-info.json"
    fi
    (cd "$out/compiled" && find . -type f | LC_ALL=C sort | while IFS= read -r f; do
        printf '%s\t%s\n' "${f#./}" "$(file -b "$f")"
    done) > "$out/compiled-files.txt"
    {
        echo "Captured $(date -u '+%Y-%m-%dT%H:%M:%SZ') by tools/capture-ios-single-icon.sh"
        echo "xcodebuild -version: $(xcodebuild -version | tr '\n' ' ')"
        echo "actool: $ACTOOL"
        echo "exit status: $status"
        echo "command: actool ${args[*]}"
    } | sed "s#$ROOT/##g" > "$out/CAPTURE.txt"
    echo "$name: exit $status, $(wc -l < "$out/compiled-files.txt" | tr -d ' ') files"
done

echo "Captured. Review tests/ref_samples/ios_single_output/*/CAPTURE.txt, then commit tests/ref_samples."
