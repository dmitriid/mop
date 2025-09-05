#!/bin/bash
set -e

echo "🔨 Building MOP.app for macOS..."

# Clean previous builds
echo "🧹 Cleaning previous builds..."
rm -rf MOP.app/Contents/MacOS/mop
rm -rf target/

# Build the Rust binary in release mode
echo "⚙️  Building Rust binary..."
cargo build --release

# Copy binary to app bundle
echo "📦 Creating app bundle..."
cp target/release/mop MOP.app/Contents/MacOS/

# Make binary executable
chmod +x MOP.app/Contents/MacOS/mop

# Check if we should code sign
if [ "$1" == "--sign" ]; then
    echo "✍️  Code signing the app..."
    
    # Sign the binary first
    codesign --force --options runtime \
        --entitlements entitlements.plist \
        --sign "Developer ID Application" \
        MOP.app/Contents/MacOS/mop
    
    # Sign the app bundle
    codesign --force --options runtime \
        --entitlements entitlements.plist \
        --sign "Developer ID Application" \
        MOP.app
    
    echo "✅ App signed successfully"
else
    echo "⚠️  App not code signed. Use --sign flag to sign for distribution."
    echo "   For development, you can also use ad-hoc signing:"
    echo "   codesign --force --deep --sign - MOP.app"
fi

echo ""
echo "✅ MOP.app created successfully!"
echo ""
echo "🚀 To run the app:"
echo "   open MOP.app"
echo ""
echo "🔧 To debug network issues:"
echo "   ./MOP.app/Contents/MacOS/mop debug"
echo ""
echo "💡 If permission dialog doesn't appear:"
echo "   1. Go to System Preferences > Security & Privacy > Privacy > Local Network"
echo "   2. Add MOP.app and check the box"
echo "   3. Restart the app"
echo ""