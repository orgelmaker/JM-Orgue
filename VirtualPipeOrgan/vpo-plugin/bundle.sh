#!/bin/bash
# Bundelt de gecompileerde plugin in VST3-bundle + CLAP single-file.
# Aanroepen vanuit de workspace-root, NA `cargo build --release -p vpo-plugin`.

set -e

WORKSPACE_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET_DIR="$WORKSPACE_ROOT/target/release"
BUNDLE_OUT="$WORKSPACE_ROOT/target/bundled"

DLL="$TARGET_DIR/vpo_plugin.dll"

if [ ! -f "$DLL" ]; then
  echo "❌ $DLL bestaat niet. Bouw eerst met: cargo build --release -p vpo-plugin"
  exit 1
fi

mkdir -p "$BUNDLE_OUT"

# ============ VST3 bundle (Windows) ============
# Structuur volgens VST3 spec:
#   JM-Orgue.vst3/Contents/x86_64-win/JM-Orgue.vst3
VST3_BUNDLE="$BUNDLE_OUT/JM-Orgue.vst3"
VST3_INNER_DIR="$VST3_BUNDLE/Contents/x86_64-win"
mkdir -p "$VST3_INNER_DIR"
cp "$DLL" "$VST3_INNER_DIR/JM-Orgue.vst3"

# ============ CLAP single-file ============
cp "$DLL" "$BUNDLE_OUT/JM-Orgue.clap"

echo "✅ Plugin bundles aangemaakt in $BUNDLE_OUT/"
echo ""
echo "   VST3:  $VST3_BUNDLE"
echo "   CLAP:  $BUNDLE_OUT/JM-Orgue.clap"
echo ""
echo "Om te installeren:"
echo "  VST3:  kopieer JM-Orgue.vst3 (de map!) naar:"
echo "         Windows:  C:\\Program Files\\Common Files\\VST3\\"
echo "         (of voeg map toe als VST3-search path in je DAW)"
echo "  CLAP:  kopieer JM-Orgue.clap naar:"
echo "         Windows:  C:\\Program Files\\Common Files\\CLAP\\"
