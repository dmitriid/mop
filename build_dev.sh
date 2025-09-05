#!/bin/bash
set -e

echo "🔨 Building MOP.app for development..."

# Build the app bundle
./build_app.sh

# Ad-hoc sign for development (this usually triggers permission dialogs)
echo "🔒 Development signing..."
codesign --force --deep --sign - MOP.app

echo ""
echo "✅ Development version ready!"
echo ""
echo "🚀 To run:"
echo "   open MOP.app"
echo ""
echo "🔍 To debug:"
echo "   ./MOP.app/Contents/MacOS/mop debug"
echo ""
echo "💡 The app should now properly request Local Network permission."
echo ""