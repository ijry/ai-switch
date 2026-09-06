#!/usr/bin/env bash
# AppImage Wayland compatibility hook.
#
# The default linuxdeploy-based AppImage bundles libwayland-client.so from the
# build host (Ubuntu 22.04).  On distros with a newer Mesa stack (CachyOS,
# Arch, Fedora 41+, etc.) the bundled library is ABI-incompatible and causes
# EGL display creation to fail with `EGL_BAD_PARAMETER`, producing a blank
# window or an immediate abort.
#
# This hook forces the AppImage to load the host's libwayland-client.so
# instead, and sets DESKTOPINTEGRATION so that GTK picks up the system
# portal / file chooser on Wayland.
#
# Placed in appimage/hooks/ and sourced by the patched AppRun before the
# wrapped binary starts.

export DESKTOPINTEGRATION=1

# Only set LD_PRELOAD if the caller hasn't already.
if [ -z "${LD_PRELOAD:-}" ]; then
    for lib in \
        /usr/lib64/libwayland-client.so.0 \
        /usr/lib/libwayland-client.so.0 \
        /usr/lib/x86_64-linux-gnu/libwayland-client.so.0 \
        /usr/lib/aarch64-linux-gnu/libwayland-client.so.0; do
        if [ -f "$lib" ]; then
            export LD_PRELOAD="$lib"
            break
        fi
    done
fi
