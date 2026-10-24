# Nozzle video practice — experimental OpenFOAM v2606 case

This directory preserves the historical command-line nozzle reference. The
desktop tutorial-specific action has been removed: use the general **Model →
Flow configuration / Generate mesh from sketch** tools, which mesh the actual
CAD contour and persist independent materials and per-segment boundaries. See
resources/flow/README.md. The historical preparer still produces an independent
axisymmetric case for comparison. Existing folders are never overwritten. Python 3
(standard library), OpenFOAM v2606 and its `wmake` compiler are required. Use a
destination without whitespace, as required by this build procedure.

The first Run creates/checks the mesh, initializes the fields and builds the
separate solver below. Later runs retain existing solution fields. Ordinary
cavity projects do not opt into this initialization or solver. OpenFOAM case
sketches allow selection, zoom and panning, while geometry edits are protected
because changing a sketch alone does not regenerate a compatible CFD mesh.

## Reference and reconstruction

The supplied 19:16 tutorial is the source for `reference.json`. Its final
material edit at 10:04–10:05 sets molecular weight **24 kg/kmol**, Cp 1501,
viscosity 1.7894e-5 and conductivity 0.0242. The final Operating Conditions
edit at approximately 12:10–12:12 sets operating pressure **0 Pa**.
Earlier values visible during editing are not the accepted final settings.
Inlet total pressure is 2.58 MPa, inlet total temperature 2800 K and exterior
pressure 30 kPa. The 100/52/154 mm geometry dimensions are heights from the
axis, rather than full diameters.

The final straight inlet is 78.23 mm. The divergent straight portion has a
**horizontal** length of 578 mm. The reconstruction retains the last visible
89.42/22.81 mm circular fillets while solving the final dimensions. This is
an explicit assumption: the original CAD and Fluent project were not supplied.
The external domain extends 3500 mm downstream and 2000 mm above the nozzle
exit. `geometry/reconstruction.json` records the resulting tangencies.

The CFD model uses a 5° wedge, ideal gas, energy and Spalart–Allmaras with an
adiabatic no-slip wall. This structured wedge replaces the tutorial's
triangular mesh. Kurganov/Minmod local time stepping replaces Fluent's
density-based implicit Roe scheme; the numerical methods are not identical.
Version 3 explicitly enforces **no swirl**, matching Fluent's axisymmetric
selection. OpenFOAM wedges retain three solution components and can represent
swirl; geometry alone does not impose zero azimuthal momentum. The helper
keeps internal mid-plane Uz exactly zero after each momentum update, while
the wedge boundary transformation retains the proper radial face components.
It rejects restarts containing azimuthal velocity instead of silently
reusing the energy of a contaminated earlier run.
The initial quasi-one-dimensional flow is a starting estimate, not a result.

The pressure outlet fixes pressure only where the outward normal velocity is
subsonic and extrapolates pressure for supersonic outflow. The cold farfield
reservoir uses an outgoing characteristic to determine inflow speed, so an
extrapolated hot-jet velocity cannot exhaust its specified total enthalpy.
Boundary turbulence inflow values use the local density to enforce the
specified turbulent-viscosity ratio. See the generated case README and source
for these translations of the tutorial's boundary conditions.

## Validation status

The preparer, mesh, initialization, native solver compilation and GUI execution
have been exercised on macOS with v2606. The default mesh has 8660 cells and
basic `checkMesh` passes. Advanced topology/geometry checks flag small cell
determinants in the anisotropic wall layers; this remains a mesh-quality caveat.

**The compressible practice is not yet validated or complete.** The earlier
v2 run retained positive temperature and density but had material boundary
mass imbalance and developed unintended azimuthal velocity. It is diagnostic
only; v3 requires a fresh no-swirl case. Reaching an iteration count or observing
zero diagonal solver residuals is not proof of steady convergence. Inspect
`postProcessing/mass*`, extrema, exit/axis profiles, field changes between
checkpoints and `wallResolution`, then establish mesh dependence. Do not use
the interim contours as validated engineering results.

`python3 test_prepare.py` checks reconstruction, parameter propagation,
boundary definitions and preservation of existing destinations. The normal
Vortia test suite checks initialization/resume isolation, backend compilation,
result selection and sketch persistence/navigation. These software checks do
not replace CFD validation.

`python3 validate_fields.py CASE/TIME` checks finite positive internal pressure,
temperature and density, together with zero mid-plane azimuthal velocity. A
fresh native 120-step v3 run passed these checks on all 8660 cells; the earlier
v2 checkpoint was correctly rejected for its nonzero Uz. This is an initial
admissibility check, not a convergence result.

## License

`solver/` is an independent GPL-3.0-or-later derivative of OpenFOAM v2606
`rhoCentralFoam`, build `_481094fdf3`. Its original notices and complete
`LICENSE.md` are retained, including in each prepared case. It is built as a
separate executable against the matching OpenFOAM libraries; it is not linked
into Vortia's MIT core. See [solver/README.md](solver/README.md).
