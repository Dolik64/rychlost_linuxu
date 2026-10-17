#!/bin/sh
set -eu
cd "$(dirname "$0")"
mkdir -p build/PopSpace.app/Contents/MacOS
mkdir -p build/PopSpace.app/Contents/Resources
cp THIRD-PARTY-NOTICES.txt build/PopSpace.app/Contents/Resources/
xcrun clang -Wall -Wextra -Wno-deprecated-declarations -fobjc-arc -framework Cocoa -framework Carbon -framework ApplicationServices PopSpace.m -o build/PopSpace.app/Contents/MacOS/PopSpace
cat > build/PopSpace.app/Contents/Info.plist <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>cz.jirka.PopSpace</string>
<key>CFBundleName</key><string>PopSpace</string>
<key>CFBundleExecutable</key><string>PopSpace</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>1</string>
<key>CFBundleShortVersionString</key><string>0.1</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
codesign --force --sign - --identifier cz.jirka.PopSpace build/PopSpace.app
codesign --verify --strict build/PopSpace.app
build/PopSpace.app/Contents/MacOS/PopSpace --probe
