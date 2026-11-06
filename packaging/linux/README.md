# Linux AppImage packaging

`create_appimage.sh` runs `linuxdeploy` plus its Qt plugin to stage the binary, bundled shared libraries and Qt xcb platform plugin in an AppDir. It then **replaces linuxdeploy's AppRun symlink** with the executable POSIX shell launcher in `AppRun`, which sets Qt plugin and library search paths relative to the mounted AppDir.

The AppDir is only converted into an AppImage **after** this replacement, by a versioned appimagetool whose downloaded SHA-256 is checked. Do not restore `--output appimage` to the first linuxdeploy step: it would package the symlink before our launcher was installed.

The release workflow checks that `libqxcb.so` exists, validates the launcher and starts a genuine `QApplication` and `QWidget` inside the built AppImage using Xvfb (`--verify-qt-platform`). A translation check with `QCoreApplication` alone is not enough: it cannot detect a missing Qt platform plugin path.

The AppImage's default QPA backend is `xcb`, which works on X11 and Wayland desktops with XWayland installed. Users can explicitly override `QT_QPA_PLATFORM`. The build currently does **not** bundle the native Wayland QPA plugin or an OpenFOAM runtime; do not claim otherwise.

The workflow writes the checksum from inside `dist`, so `sha256sum -c Vortia-<version>-linux-x86_64.AppImage.sha256` works when both release files are downloaded into the same directory. On a regular push or PR it produces a CI artifact only; a published GitHub Release (or a manual workflow dispatch with an explicit tag) attaches assets to the Release.
