#!/usr/bin/env bash

set -euo pipefail

if [ "$#" -ne 3 ]; then
  echo "Usage: $0 <arm64-binary> <x86_64-binary> <output-directory>" >&2
  exit 2
fi

arm64_binary="$1"
x86_64_binary="$2"
output_directory="$3"
work_directory="$(mktemp -d)"

cleanup() {
  rm -rf "$work_directory"
}
trap cleanup EXIT

mkdir -p "$output_directory"

arm64_unsigned="$work_directory/flix-desktop-arm64"
x86_64_unsigned="$work_directory/flix-desktop-x86_64"
universal_directory="target/universal-apple-darwin/release"
universal_binary="$universal_directory/flix-desktop"

cp "$arm64_binary" "$arm64_unsigned"
cp "$x86_64_binary" "$x86_64_unsigned"

# rustc gives each thin Mach-O file a linker signature. These signatures are
# invalid after lipo joins the files and can make dyld stall before main runs.
codesign --remove-signature "$arm64_unsigned" 2>/dev/null || true
codesign --remove-signature "$x86_64_unsigned" 2>/dev/null || true

mkdir -p "$universal_directory"
lipo -create \
  "$arm64_unsigned" \
  "$x86_64_unsigned" \
  -output "$universal_binary"
chmod +x "$universal_binary"
codesign --force --sign - "$universal_binary"

cargo packager \
  --release \
  --formats app \
  --target universal-apple-darwin \
  --binaries-dir "$universal_directory" \
  --out-dir "$output_directory"

app_path="$(find "$output_directory" -maxdepth 1 -type d -name '*.app' -print -quit)"
if [ -z "$app_path" ]; then
  echo "cargo-packager did not create a macOS app bundle." >&2
  exit 1
fi

# An ad-hoc bundle signature keeps the unsigned development release valid.
# A public build still needs Developer ID signing and notarization to pass
# Gatekeeper without a user quarantine bypass.
codesign --force --deep --sign - "$app_path"
codesign --verify --deep --strict --verbose=2 "$app_path"

dmg_root="$work_directory/dmg-root"
mkdir -p "$dmg_root"
ditto "$app_path" "$dmg_root/Flix.app"
ln -s /Applications "$dmg_root/Applications"

dmg_path="$output_directory/Flix-macOS-universal.dmg"
hdiutil create \
  -volname Flix \
  -srcfolder "$dmg_root" \
  -format UDZO \
  -ov \
  "$dmg_path"

echo "Created $dmg_path"
