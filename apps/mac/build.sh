#!/bin/sh
# Builds the Mac app: the Rust core as a static library, its Swift bindings,
# the Swift package, and build/AAW.app around the result. The bundle holds the
# `daw` binary and the libraries the two link, so it runs from any folder.
#
#   ./build.sh          release build
#   ./build.sh debug    debug Swift build (the Rust core is always optimized)
#   ./build.sh test     builds the core and bindings, then runs `swift test`
#   ./build.sh dist     release build, zipped as build/AAW-VERSION.zip to share
#
# The bundle is signed ad hoc, or with the certificate AAW_SIGN_IDENTITY names,
# such as "Developer ID Application: NAME (TEAM)". With AAW_NOTARY_PROFILE, a
# keychain profile made by `xcrun notarytool store-credentials`, `dist` has
# Apple notarize the app and staples the ticket to it.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
engine="$here/../../engine"
mode=${1:-release}
generated="$here/Generated"
identity=${AAW_SIGN_IDENTITY:--}

case "$mode" in
release | debug | test | dist) ;;
*)
    echo "usage: build.sh [release | debug | test | dist]" >&2
    exit 2
    ;;
esac
if [ "$mode" = dist ] && [ -n "${AAW_NOTARY_PROFILE:-}" ] && [ "$identity" = - ]; then
    echo "Apple notarizes only what a Developer ID signed: set AAW_SIGN_IDENTITY" >&2
    exit 2
fi

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

config=$mode
if [ "$mode" = dist ]; then config=release; fi
swift build --package-path "$here" -c "$config"
bin=$(swift build --package-path "$here" -c "$config" --show-bin-path)

# `daw` is built by itself, as the tests build it, so the bundle's copy is the
# binary they drive.
(cd "$engine" && cargo build --release -p aaw-cli)

app="$here/build/AAW.app"
contents="$app/Contents"
frameworks="$contents/Frameworks"
licenses="$contents/Resources/Licenses"
rm -rf "$app"
mkdir -p "$contents/MacOS" "$contents/Helpers" "$frameworks" "$licenses"
cp "$bin/AAW" "$contents/MacOS/AAW"
cp "$engine/target/release/daw" "$contents/Helpers/daw"
cp "$here/Info.plist" "$contents/Info.plist"

# Runs a command that has things to say when it works too, and shows them only
# when it fails.
quiet() {
    said=$("$@" 2>&1) || {
        echo "$said" >&2
        exit 1
    }
}

# What a binary links, but for itself.
links() {
    otool -L "$1" | awk 'NR > 1 { print $1 }'
}

# embed FILE PREFIX: the libraries FILE links from outside the system, such as
# Homebrew's libsndfile, are copied into Frameworks with their licenses, and
# FILE is pointed at the copies, which it finds at PREFIX.
embed() {
    links "$1" | while read -r lib; do
        case "$lib" in /usr/lib/* | /System/* | @*) continue ;; esac
        name=$(basename "$lib")
        if [ ! -e "$frameworks/$name" ]; then
            cp -L "$lib" "$frameworks/$name"
            chmod 644 "$frameworks/$name"
            quiet install_name_tool -id "@rpath/$name" "$frameworks/$name"
            package=$(dirname "$(dirname "$lib")")
            for notice in "$package"/COPYING* "$package"/LICENSE*; do
                if [ -f "$notice" ]; then cp "$notice" "$licenses/${name%%.*}-$(basename "$notice")"; fi
            done
        fi
        quiet install_name_tool -change "$lib" "$2/$name" "$1"
    done
}

embed "$contents/MacOS/AAW" "@executable_path/../Frameworks"
embed "$contents/Helpers/daw" "@executable_path/../Frameworks"
# The copies link one another: go round until a pass brings no more in.
copies() {
    find "$frameworks" -name '*.dylib' | wc -l | tr -d ' '
}
count=-1
until [ "$count" -eq "$(copies)" ]; do
    count=$(copies)
    find "$frameworks" -name '*.dylib' | while read -r lib; do embed "$lib" "@loader_path"; done
done

# Nothing in the bundle may link outside it and the system, or be built for a
# newer macOS than the app says it runs on.
floor=$(/usr/libexec/PlistBuddy -c "Print :LSMinimumSystemVersion" "$contents/Info.plist")
find "$contents/MacOS" "$contents/Helpers" "$frameworks" -type f | while read -r file; do
    outside=$(links "$file" | grep -v -e '^/usr/lib/' -e '^/System/' -e '^@' || true)
    if [ -n "$outside" ]; then
        echo "$file still links outside the bundle: $outside" >&2
        exit 1
    fi
    built=$(vtool -show-build "$file" | awk '$1 == "minos" { print $2 }')
    newest=$(printf '%s\n%s\n' "$floor" "$built" | sort -t. -k1,1n -k2,2n | tail -1)
    if [ "$newest" != "$floor" ]; then
        echo "$file needs macOS $built, and Info.plist says the app runs on $floor" >&2
        exit 1
    fi
done

# A certificate signs with the hardened runtime and a timestamp, which
# notarizing asks for. Code inside the bundle is signed before the bundle.
sign() {
    if [ "$identity" = - ]; then
        quiet codesign --force --sign - "$@"
    else
        quiet codesign --force --sign "$identity" --options runtime --timestamp "$@"
    fi
}
find "$frameworks" -name '*.dylib' | while read -r lib; do sign "$lib"; done
sign --identifier local.aaw.daw "$contents/Helpers/daw"
sign "$app"
codesign --verify --strict --deep "$app"
"$contents/Helpers/daw" --help >/dev/null

if [ "$mode" = dist ]; then
    version=$(/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" "$contents/Info.plist")
    zip="$here/build/AAW-$version.zip"
    rm -f "$zip"
    ditto -c -k --keepParent "$app" "$zip"
    if [ -n "${AAW_NOTARY_PROFILE:-}" ]; then
        xcrun notarytool submit "$zip" --keychain-profile "$AAW_NOTARY_PROFILE" --wait
        # The ticket goes into the bundle, so the zip is made again with it.
        xcrun stapler staple "$app"
        rm "$zip"
        ditto -c -k --keepParent "$app" "$zip"
    else
        echo "Not notarized: another Mac refuses to open it until its user allows it" >&2
    fi
    echo "$zip"
else
    echo "$app"
fi
