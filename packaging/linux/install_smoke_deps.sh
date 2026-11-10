#!/bin/sh
# Install the minimum a stock desktop install already has (X server, Mesa,
# fontconfig) into a bare distro container, so the AppImage smoke test runs
# against a "clean machine". Deliberately does NOT install Qt, xcb-util
# libraries (libxcb-cursor0 & co.), libopengl0 or a newer libstdc++: the
# AppImage must carry those itself.
set -eu

if command -v apt-get >/dev/null 2>&1; then
  export DEBIAN_FRONTEND=noninteractive
  # The Ubuntu archive regularly drops connections from CI runners.
  echo 'Acquire::Retries "8"; Acquire::http::Timeout "30";' > /etc/apt/apt.conf.d/80-vortia-retries
  attempt=1
  until apt-get update && apt-get install -y --no-install-recommends \
    xvfb xauth libgl1 libegl1 libgl1-mesa-dri libglx-mesa0 \
    fontconfig fonts-dejavu-core file binutils ca-certificates; do
    [ "$attempt" -ge 5 ] && exit 1
    attempt=$((attempt + 1))
    sleep 20
  done
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
    fontconfig dejavu-fonts file binutils which findutils gawk
else
  echo "Unsupported distribution: no apt-get, dnf, pacman or zypper" >&2
  exit 1
fi
