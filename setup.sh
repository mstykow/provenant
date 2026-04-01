#!/bin/bash
#
# Setup script for Provenant development
#
# This script initializes git submodules and, when Node tooling is present, installs git hooks.
# License rules and licenses are in the reference/scancode-toolkit submodule.
#
# You only need to run this script if you:
# - Want to install git hooks after installing Node dependencies
# - Want to update to the latest license rules/licenses

set -e

echo "Initializing submodules..."
git submodule update --init --filter=blob:none

echo ""
echo "Installing git hooks..."
if [ -x node_modules/.bin/lefthook ]; then
    npm run hooks:install
    echo "✅ Lefthook hooks installed"
else
    echo "⚠️  Lefthook is not installed yet. Run npm install first (it will auto-install hooks), or run:"
    echo "   npm install"
    echo "   npm run hooks:install"
fi

echo ""
echo "Setup complete."
