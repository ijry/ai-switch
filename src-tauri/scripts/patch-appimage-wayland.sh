#!/usr/bin/env bash
# patch-appimage-wayland.sh — Post-build fix for the Tauri AppImage.
#
# Problem:  linuxdeploy bundles the build host's libwayland-client.so into the
#           AppImage.  On distros with a newer Mesa stack (CachyOS, Arch,
#           Fedora 41+, …) the bundled library is ABI-incompatible, causing
#           `EGL_BAD_PARAMETER` and a blank/aborted window.
#
# Solution: After `tauri build` produces the AppImage, this script:
#   1. Finds the generated *.AppDir
#   2. Deletes the bundled libwayland*.so* files
#   3. Copies the Wayland-compat hook into the AppDir
#   4. Patches AppRun to source the hook before executing the wrapped binary
#   5. Re-packs the AppImage using the linuxdeploy from Tauri's cargo cache
#
# Usage (called from CI after `tauri build`):
#   bash src-tauri/scripts/patch-appimage-wayland.sh
#
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUNDLE_DIR="$REPO_ROOT/src-tauri/target/release/bundle/appimage"

# ── 1. Locate the AppDir ─────────────────────────────────────────────────────
APPDIR=$(find "$BUNDLE_DIR" -maxdepth 1 -type d -name '*.AppDir' | head -1)
if [ -z "$APPDIR" ]; then
    echo "[patch-appimage] No AppDir found in $BUNDLE_DIR — skipping patch." >&2
    exit 0
fi

echo "[patch-appimage] Patching $APPDIR"

# ── 2. Remove bundled libwayland ─────────────────────────────────────────────
WL_LIB_DIR="$APPDIR/usr/lib"
if [ -d "$WL_LIB_DIR" ]; then
    removed=0
    for f in "$WL_LIB_DIR"/libwayland*.so*; do
        [ -e "$f" ] || continue
        rm -f "$f"
        removed=$((removed + 1))
    done
    echo "[patch-appimage] Removed $removed bundled libwayland file(s)"
fi

# ── 3. Install the compatibility hook ────────────────────────────────────────
HOOKS_DIR="$APPDIR/apprun-hooks"
mkdir -p "$HOOKS_DIR"
cp "$REPO_ROOT/src-tauri/appimage/apprun-wayland-compat.sh" "$HOOKS_DIR/wayland-compat.sh"
chmod +x "$HOOKS_DIR/wayland-compat.sh"

# ── 4. Patch AppRun to source the hook ───────────────────────────────────────
APPRUN="$APPDIR/AppRun"
if [ -f "$APPRUN" ] && ! grep -q 'wayland-compat.sh' "$APPRUN"; then
    # Insert the source line right after the shebang / initial blank lines.
    # We look for the first non-blank, non-comment line and insert before it.
    TMPFILE=$(mktemp)
    awk '
        BEGIN { inserted = 0 }
        !inserted && /^[^#]/ && NF > 0 {
            print "# --- Wayland compatibility hook (auto-patched) ---"
            print "if [ -f \"$APPDIR/apprun-hooks/wayland-compat.sh\" ]; then"
            print "    . \"$APPDIR/apprun-hooks/wayland-compat.sh\""
            print "fi"
            print "# --- end hook ---"
            inserted = 1
        }
        { print }
    ' APPDIR="$APPDIR" "$APPRUN" > "$TMPFILE"
    mv "$TMPFILE" "$APPRUN"
    chmod +x "$APPRUN"
    echo "[patch-appimage] Patched AppRun to source wayland-compat hook"
else
    echo "[patch-appimage] AppRun already patched or not found — skipping"
fi

# ── 5. Re-pack the AppImage ──────────────────────────────────────────────────
# The linuxdeploy binary is cached by tauri-cli in the cargo target dir.
LINUXDEPLOY=$(find "$REPO_ROOT/src-tauri/target" -name 'linuxdeploy' -type f 2>/dev/null | head -1)
if [ -z "$LINUXDEPLOY" ]; then
    # Fallback: also check the tauri cache in ~/.cache
    LINUXDEPLOY=$(find "${XDG_CACHE_HOME:-$HOME/.cache}/tauri" -name 'linuxdeploy' -type f 2>/dev/null | head -1)
fi

if [ -n "$LINUXDEPLOY" ] && [ -x "$LINUXDEPLOY" ]; then
    # Find the existing AppImage to extract its name for the output.
    OLD_APPIMAGE=$(find "$BUNDLE_DIR" -maxdepth 1 -name '*.AppImage' | head -1)
    if [ -n "$OLD_APPIMAGE" ]; then
        APPIMAGE_NAME=$(basename "$OLD_APPIMAGE")
    else
        APPIMAGE_NAME="app.AppImage"
    fi
    OUTPUT="$BUNDLE_DIR/$APPIMAGE_NAME"

    echo "[patch-appimage] Re-packing with $LINUXDEPLOY"
    "$LINUXDEPLOY" \
        --appdir "$APPDIR" \
        --output appimage \
        --deploy-library-path "$WL_LIB_DIR" \
        2>&1 | tail -5

    # linuxdeploy writes the new AppImage next to the AppDir.  Move it into
    # place if the name doesn't match the original.
    NEW_APPIMAGE=$(find "$BUNDLE_DIR" -maxdepth 1 -name '*.AppImage' -newer "$APPDIR" | head -1)
    if [ -n "$NEW_APPIMAGE" ] && [ "$NEW_APPIMAGE" != "$OUTPUT" ]; then
        rm -f "$OLD_APPIMAGE"
        mv "$NEW_APPIMAGE" "$OUTPUT"
    fi
    echo "[patch-appimage] Done — output: $OUTPUT"
else
    echo "[patch-appimage] linuxdeploy not found — AppDir patched but NOT re-packed." >&2
    echo "[patch-appimage] The AppDir at $APPDIR has been modified in-place." >&2
    echo "[patch-appimage] To re-pack manually, run linuxdeploy against the AppDir." >&2
    exit 1
fi
