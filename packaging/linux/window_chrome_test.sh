#!/usr/bin/env bash
# Exercise Vortia's integrated top bar with a real window manager: the window
# must move when its bar is dragged, resize from an edge, maximize on a
# double click and close from its own button.
# Usage: window_chrome_test.sh <Vortia.AppImage>
# Needs: Xvfb, openbox, xdotool. Run on a machine with FUSE (not a container).
set -uo pipefail

image="$(readlink -f "$1")"
chmod +x "$image"
work="$(mktemp -d)"
fail=0
bad() { echo "FAIL: $*"; fail=1; }

export DISPLAY=":97"
Xvfb "$DISPLAY" -screen 0 1600x1000x24 >/dev/null 2>&1 &
xvfb_pid=$!
sleep 2
openbox >/dev/null 2>&1 &
wm_pid=$!
sleep 1

env -u LD_LIBRARY_PATH -u QT_PLUGIN_PATH -u QT_QPA_PLATFORM "HOME=$work" VORTIA_NO_STARTUP_DIALOG=1 \
  VORTIA_WINDOW_BUTTONS="appmenu:minimize,maximize,close" "$image" >"$work/app.log" 2>&1 &
app_pid=$!
cleanup() { kill "$app_pid" "$wm_pid" "$xvfb_pid" 2>/dev/null; rm -rf "$work"; }
trap cleanup EXIT

window=""
for _ in $(seq 1 60); do
  window="$(xdotool search --onlyvisible --name 'Vortia' 2>/dev/null | head -n 1)"
  [[ -n "$window" ]] && break
  sleep 0.5
done
[[ -n "$window" ]] || { echo "FAIL: main window never appeared"; tail -n 20 "$work/app.log"; exit 1; }
sleep 2

geometry() { # prints: X Y WIDTH HEIGHT
  eval "$(xdotool getwindowgeometry --shell "$window")"
  echo "$X $Y $WIDTH $HEIGHT"
}
# Start from a known, non-maximized state.
xdotool windowsize "$window" 1100 700 windowmove "$window" 100 100
sleep 1
read -r x0 y0 w0 h0 < <(geometry)
echo "start: $x0 $y0 ${w0}x${h0}"

echo "== no system title bar =="
# With client-side decorations the frame extents are zero on all sides.
extents="$(xprop -id "$window" _NET_FRAME_EXTENTS 2>/dev/null | sed 's/.*= //')"
echo "frame extents: ${extents:-none}"
case "$extents" in ""|"0, 0, 0, 0") echo "OK" ;; *) bad "the window still has a system frame" ;; esac

echo "== drag the empty part of the top bar =="
xdotool mousemove $((x0 + w0 / 2)) $((y0 + 22)) mousedown 1
for step in 1 2 3 4 5 6; do xdotool mousemove_relative 30 20; sleep 0.1; done
xdotool mouseup 1
sleep 1
read -r x1 y1 w1 h1 < <(geometry)
echo "after drag: $x1 $y1 ${w1}x${h1}"
if (( x1 - x0 > 100 && y1 - y0 > 60 )) && (( w1 == w0 && h1 == h0 )); then echo "OK: moved"; else bad "window did not follow the drag"; fi

echo "== resize from the right edge =="
xdotool mousemove $((x1 + w1 - 2)) $((y1 + h1 / 2)) mousedown 1
for step in 1 2 3 4 5; do xdotool mousemove_relative 30 0; sleep 0.1; done
xdotool mouseup 1
sleep 1
read -r x2 y2 w2 h2 < <(geometry)
echo "after resize: $x2 $y2 ${w2}x${h2}"
if (( w2 - w1 > 100 && h2 == h1 )); then echo "OK: resized"; else bad "window did not resize from its edge"; fi

echo "== double click on the top bar maximizes, and restores =="
xdotool mousemove $((x2 + w2 / 2)) $((y2 + 22)) click --repeat 2 --delay 80 1
sleep 1.5
read -r x3 y3 w3 h3 < <(geometry)
echo "after double click: $x3 $y3 ${w3}x${h3}"
if (( w3 >= 1580 && h3 >= 950 )); then echo "OK: maximized"; else bad "double click did not maximize"; fi
xdotool mousemove $((x3 + w3 / 2)) $((y3 + 22)) click --repeat 2 --delay 80 1
sleep 1.5
read -r x4 y4 w4 h4 < <(geometry)
echo "after second double click: $x4 $y4 ${w4}x${h4}"
if (( w4 < 1500 )); then echo "OK: restored"; else bad "second double click did not restore"; fi

echo "== the close button of the bar closes the application =="
# Rightmost button: 8 px margin + half of a 26 px button + its 3 px margin.
xdotool mousemove $((x4 + w4 - 8 - 3 - 13)) $((y4 + 22)) click 1
for _ in $(seq 1 20); do kill -0 "$app_pid" 2>/dev/null || break; sleep 0.5; done
if kill -0 "$app_pid" 2>/dev/null; then bad "application still running after clicking close"; else echo "OK: closed"; fi

echo
[[ "$fail" -eq 0 ]] && echo "WINDOW CHROME TEST PASSED" || { echo "WINDOW CHROME TEST FAILED"; tail -n 15 "$work/app.log"; }
exit "$fail"
