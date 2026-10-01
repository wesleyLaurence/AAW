#!/bin/sh
# Builds the Mac app: the Rust core as a static library, its Swift bindings,
# the Swift package, and build/AAW.app around the result.
#
#   ./build.sh          release build
#   ./build.sh debug    debug Swift build (the Rust core is always optimized)
#   ./build.sh test     builds the core and bindings, then runs `swift test`
set -eu

here=$(cd "$(dirname "$0")" && pwd)
engine="$here/../../engine"
mode=${1:-release}
generated="$here/Generated"

if [ -z "${SNDFILE_LIB_DIR:-}" ] && command -v brew >/dev/null; then
    SNDFILE_LIB_DIR="$(brew --prefix libsndfile)/lib"
    export SNDFILE_LIB_DIR
fi

(cd "$engine" && cargo build --release -p aaw-ffi)

# Bindings come from the built library, so they cannot drift from it.
bindings="$generated/bindings"
rm -rf "$bindings" "$generated/headers" "$generated/aaw_ffiFFI.xcframework"
mkdir -p "$bindings" "$generated/headers"
(cd "$engine" && cargo run -q --release -p aaw-ffi --features bindgen --bin uniffi-bindgen -- \
    generate target/release/libaaw_ffi.dylib --language swift --out-dir "$bindings")
cp "$bindings/aaw_ffiFFI.h" "$generated/headers/"
cp "$bindings/aaw_ffiFFI.modulemap" "$generated/headers/module.modulemap"
xcodebuild -create-xcframework \
    -library "$engine/target/release/libaaw_ffi.a" -headers "$generated/headers" \
    -output "$generated/aaw_ffiFFI.xcframework" >/dev/null
mkdir -p "$here/Sources/AAWCore"
cp "$bindings/aaw_ffi.swift" "$here/Sources/AAWCore/aaw_ffi.swift"

if [ "$mode" = test ]; then
    exec swift test --package-path "$here"
fi

swift build --package-path "$here" -c "$mode"
bin=$(swift build --package-path "$here" -c "$mode" --show-bin-path)

app="$here/build/AAW.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS"
cp "$bin/AAW" "$app/Contents/MacOS/AAW"
cp "$here/Info.plist" "$app/Contents/Info.plist"
codesign --force --sign - "$app" >/dev/null 2>&1
echo "$app"
