#!/bin/sh
# Install the minimum a stock desktop install already has (X server, Mesa,
# fontconfig) into a bare distro container, so the AppImage smoke test runs
# against a "clean machine". Deliberately does NOT install Qt, xcb-util
# libraries (libxcb-cursor0 & co.) or a newer libstdc++: the AppImage must
# carry those itself.
set -eu

if command -v apt-get >/dev/null 2>&1; then
  export DEBIAN_FRONTEND=noninteractive
  apt-get update
  apt-get install -y --no-install-recommends \
    xvfb xauth libgl1 libegl1 libgl1-mesa-dri libglx-mesa0 libopengl0 \
    fontconfig fonts-dejavu-core file binutils ca-certificates
elif command -v dnf >/dev/null 2>&1; then
  dnf install -y \
    xorg-x11-server-Xvfb xorg-x11-xauth mesa-libGL mesa-libEGL mesa-dri-drivers \
    libglvnd-opengl fontconfig dejavu-sans-fonts file binutils which findutils
elif command -v pacman >/dev/null 2>&1; then
  pacman -Syu --noconfirm --needed \
    xorg-server-xvfb xorg-xauth mesa libglvnd fontconfig ttf-dejavu file \
    binutils which
elif command -v zypper >/dev/null 2>&1; then
  zypper -n install --no-recommends \
    xorg-x11-server-Xvfb xvfb-run xauth Mesa-libGL1 Mesa-libEGL1 Mesa-dri \
    libOpenGL0 fontconfig dejavu-fonts file binutils which findutils gawk
else
  echo "Unsupported distribution: no apt-get, dnf, pacman or zypper" >&2
  exit 1
fi
