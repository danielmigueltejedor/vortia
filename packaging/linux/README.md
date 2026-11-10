# Linux AppImage packaging

## What failed in 0.2.1 and earlier

Reproduced on clean distribution containers:

1. `AppRun` was linuxdeploy's symlink to the executable: no Qt plugin path.
2. linuxdeploy skips every library on the AppImage excludelist (harfbuzz,
   freetype, fontconfig, zlib, libstdc++…). Our Qt/VTK come from conda-forge
   and need newer versions than Ubuntu 22.04/24.04 or Debian 12 ship.
3. The executable needed glibc 2.38: the conda solver picked `sysroot_linux-64`
   2.39 when unpinned.
4. VTK links `libOpenGL.so.0`, which stock Ubuntu/Debian desktops do not install.

## How the AppImage is built now

`create_appimage.sh` stages the AppDir with linuxdeploy, then:

- copies the full dependency closure from the build environment, except glibc,
  the OpenGL/driver stack and the core X11 libraries (host-only);
- moves `libstdc++`/`libgcc_s` to `usr/optional/gcc-runtime` and `libOpenGL` to
  `usr/optional/glvnd`; `AppRun` adds them only when the host's copy is older
  or missing (host graphics drivers may need a newer C++ runtime than ours);
- fails if any bundled binary needs glibc newer than 2.28;
- adds the OpenFOAM engine and the mesher from `build_openfoam_payload.sh`
  (Ubuntu 20.04 build: glibc 2.31 overall);
- installs the shell launcher `AppRun` and packs with a checksum-pinned
  appimagetool.

`AppRun` records the user's `LD_LIBRARY_PATH` and Qt variables; Vortia restores
them for every process it starts (`vortia/core/HostEnvironment.hpp`), so
OpenFOAM and Python never load the GUI's bundled libraries.

## Tests before anything is released

`smoke_test_appimage.sh` runs on bare containers of eight distributions
(`install_smoke_deps.sh` adds only an X server, Mesa and fontconfig) and, with
`--fuse`, on real Ubuntu VMs where the image file is mounted by its runtime.
It checks library resolution, the real main window and OpenGL viewport
(`--verify-startup`), a hostile `LD_LIBRARY_PATH`, the no-OpenGL message, the
desktop-launch log, the mesher and a real serial + MPI OpenFOAM run.

## Honest limits

- The user must mark the downloaded file executable; browsers strip that bit.
- A FUSE mount needs `fusermount` on the host. Without it:
  `./Vortia.AppImage --appimage-extract-and-run`.
- xcb (X11/XWayland) is the tested backend. The native Wayland plugins are
  bundled and selected only when there is no X display; that path is untested.
- The experimental compressible solver is not available in the AppImage
  (needs OpenFOAM v2606 and a compiler).
