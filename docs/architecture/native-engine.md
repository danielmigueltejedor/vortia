# vortia-engine and Vortia Native

Vortia ships two macOS editions that share everything below the interface:

| Edition | Interface | Package |
| --- | --- | --- |
| **Vortia Nodalia** | Qt 6, in the Nodalia Design Language (`src/ui`, `src/app`) | `Vortia-<version>-macos-arm64.dmg`, and the Linux AppImage |
| **Vortia Native** | SwiftUI, with the system's controls and Liquid Glass on macOS 26 (`platforms/apple`) | `Vortia-<version>-native-macos-arm64.dmg` |

The Native edition does not reimplement any engineering. It drives **vortia-engine**
(`src/engine`), a headless executable built from the same libraries as the desktop app:
`core`, `geometry`, `mesh`, `physics`, `solver` (the OpenFOAM case writer, runner and log
parser) and the VTK-based result analysis in `visualization`. A case written, meshed and
solved from either edition is the same case.

The protocol is also the seam for the Rust migration: a Rust engine that answers the same
lines can replace the C++ one under Vortia Native without the interface changing.

## Protocol (version 1)

Newline-delimited JSON on the engine's stdin and stdout. **stdout carries protocol only**;
anything for a person goes to stderr.

A request is `{"id": <int>, "cmd": <string>, …arguments}`. Every request is answered once:

```json
{"id": 4, "ok": true, "result": {…}}
{"id": 4, "ok": false, "error": "the case setup has errors; validate it first"}
```

Events have no id:

| Event | Fields | When |
| --- | --- | --- |
| `ready` | `protocol` | once, at start |
| `job` | `state` (`running` / `finished`), `phase` (`mesh` / `solve`), `ok`, `summary` | a run starts a phase, or ends |
| `progress` | `time`, `courantMean`, `courantMax`, `residuals[] {field, initial, final, iterations}` | while solving, at most every 100 ms |
| `log` | `text` | raw OpenFOAM output |

Numbers that are not finite are sent as `null`.

| Command | Arguments | Result |
| --- | --- | --- |
| `hello` | — | `protocol`, `version`, `openfoam`, `project`, `running` |
| `detect` | — | `openfoam {found, fingerprint, distribution, root, error}` |
| `create` | `path`, `name` | the project: `name`, `root`, `state`, `setup` (a lid-driven cavity) |
| `open` | `path` | the project |
| `save` | `setup`, optional `name` | `saved` |
| `validate` | `setup` | `issues[] {severity, message}`, `courant`, `reynolds` |
| `mesh` | — | `started`; then `job` and `log` events |
| `solve` | optional `processes` | `started`; then `job`, `progress` and `log` events |
| `cancel` | — | `cancelling` |
| `results` | — | `file`, `fields[]`, `bounds [[min xyz], [max xyz]]` |
| `statistics` | `field`, optional `bins` | `minimum`, `maximum`, `mean`, `standardDeviation`, `integral`, `measure`, `cells`, `excluded`, `histogram[]` |
| `line` | `field`, `start`, `end`, optional `points` | `samples[] {distance, value}` |
| `slice` | `field`, `axis` (0–2), `position` | triangles: `positions` (9 floats each), `values` (one per vertex), `range` |
| `surface` | `field` | the outer surface, as triangles |
| `sketch` | — | the project's fluid contour: `points`, `closed`, `segments[] {length, bulge, axis}`, `nozzle`; or `null` |
| `saveSketch` | `sketch` (the `.vsketch.json` format) | the contour; flow boundaries follow their segments |
| `nozzle` | `dimensions` (inlet, throat, outlet diameters; converging, diverging lengths, mm) | the contour of that nozzle |
| `flow` | — | `flow`: the compressible setup, as the desktop app's flow panel would show it |
| `saveFlow` | `flow` | the setup, normalised against the contour |
| `flowMesh` / `flowSolve` | — | `started`; Gmsh meshing, then (solve) vortiaCentralFoam, with `job`, `log` and `progress` (plus `convergence`) events |
| `section` | `options` (`SectionOptions`: axis, normal, position, reference values), `label` | `report` (the desktop app's `vortia.engineering-report`, with its SHA-256), `html`, `csv` |
| `quit` | — | the engine cancels any run and exits |

`setup` is exactly the JSON `caseSetupToJson` writes and `caseSetupFromJson` reads
(`src/physics`), so it is validated by the same strict parser as the desktop app. Closing the
engine's stdin is the same as `quit`: a client that goes away never leaves OpenFOAM running.

## Building

```bash
cmake --build build --target vortia-engine test_engine   # with VORTIA_BUILD_UI=ON
build/bin/test_engine
swift test --package-path platforms/apple                 # on macOS
VORTIA_ENGINE_PATH=$PWD/build/bin/vortia-engine swift run --package-path platforms/apple VortiaMac
```

The release workflow builds the Qt bundle with `vortia-engine` inside it, deploys its
libraries once, then `platforms/apple/build.sh` turns a copy of that bundle into
`Vortia Native.app`: same engine, libraries, OpenFOAM and mesher, with the SwiftUI interface
in place of the Qt one.

## What the first Native release does, and what it does not yet

It covers the box-domain workflow end to end: create or open a project, edit the domain, the
mesh, the fluid, the boundary conditions, time and numerics with live validation (Reynolds,
Courant, the same errors and warnings as the desktop app), mesh, solve with residuals charted as
they arrive and the log beside them, cancel, and read the result: a slice coloured by any field,
its statistics and histogram, and the profile along the vertical centre line.

It also measures a section through the result and writes the same engineering report as the
desktop app, saved as HTML, CSV or JSON.

Not yet in Native: the 2D sketch editor and nozzle workflows, compressible flow, Fluent
interop, and a 3D view (the engine already serves `surface` for it).
Those stay in Vortia Nodalia until they move to the engine.
