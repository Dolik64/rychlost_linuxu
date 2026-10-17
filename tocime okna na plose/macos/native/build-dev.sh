#!/bin/sh
set -eu
cd "$(dirname "$0")"
mkdir -p build-dev/PopSpaceDev.app/Contents/MacOS
mkdir -p build-dev/PopSpaceDev.app/Contents/Resources
cp THIRD-PARTY-NOTICES.txt build-dev/PopSpaceDev.app/Contents/Resources/
xcrun clang -Wall -Wextra -Wno-deprecated-declarations -fobjc-arc -framework Cocoa -framework Carbon -framework ApplicationServices PopSpace-dev.m -o build-dev/PopSpaceDev.app/Contents/MacOS/PopSpace
cat > build-dev/PopSpaceDev.app/Contents/Info.plist <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>cz.jirka.PopSpace.Dev</string>
<key>CFBundleName</key><string>PopSpace Dev</string>
<key>CFBundleExecutable</key><string>PopSpace</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>2</string>
<key>CFBundleShortVersionString</key><string>0.2-dev</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
codesign --force --sign - --identifier cz.jirka.PopSpace.Dev build-dev/PopSpaceDev.app
codesign --verify --strict build-dev/PopSpaceDev.app
build-dev/PopSpaceDev.app/Contents/MacOS/PopSpace --probe
