#!/bin/sh
# Assemble a minimal macOS .app bundle around the cargo binary.
#
# Usage: scripts/bundle-app.sh [debug|release] [rust-target]
#   rust-target  optional triple (e.g. aarch64-apple-darwin); when set the
#               binary is read from target/<triple>/<profile>.
# The script assumes `cargo build` has already run for the chosen profile.
set -eu

profile="${1:-debug}"
target="${2:-}"
app_name="ClaudeSessionSwitch"
bundle_identifier="com.cloudcode.sessionswitch"
root="$(cd -- "$(dirname -- "$0")/.." && pwd)"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/app/Cargo.toml" | head -1)"

if [ -n "$target" ]; then
    binary_dir="$root/app/target/$target/$profile"
else
    binary_dir="$root/app/target/$profile"
fi
binary="$binary_dir/claude-session-switch"
[ -f "$binary" ] || { echo "missing binary: $binary (run cargo build first)" >&2; exit 1; }

bundle="$binary_dir/$app_name.app"
contents="$bundle/Contents"
rm -rf "$bundle"
mkdir -p "$contents/MacOS" "$contents/Resources"
cp "$binary" "$contents/MacOS/$app_name"
cp "$root/app/resources/AppIcon.icns" "$contents/Resources/AppIcon.icns"

cat > "$contents/Info.plist" << PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>zh_CN</string>
    <key>CFBundleDisplayName</key>
    <string>$app_name</string>
    <key>CFBundleExecutable</key>
    <string>$app_name</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
    <key>CFBundleIdentifier</key>
    <string>$bundle_identifier</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>$app_name</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$version</string>
    <key>CFBundleVersion</key>
    <string>$version</string>
    <key>LSMinimumSystemVersion</key>
    <string>13.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
PLIST

# Strip extended attributes so codesign accepts the bundle, then ad-hoc sign.
xattr -cr "$bundle"
codesign --force --sign - "$bundle"
codesign --verify --strict "$bundle"
echo "$bundle"
