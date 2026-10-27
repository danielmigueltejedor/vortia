# Engineering section analysis

Results → Sample → Engineering analysis opens a single workbench for section
measurements, immutable comparison snapshots and technical reports. The Spanish
path is Resultados → Consultar → Análisis de ingeniería. Load calculated fields
first. No solver run is started by this tool.

## Workflow

1. Name the analysis and choose an X, Y or Z plane, its position in metres and
   positive normal direction. Use an interior plane rather than a boundary face.
2. Choose stored `rho` or an explicit constant density. Declare whether stored
   `p` is pressure in Pa or kinematic pressure in m²/s². Kinematic pressure is
   multiplied by the selected density at each integration point. Use the same
   absolute/gauge convention for reference pressure after conversion to Pa.
3. Set reference density, velocity and area. They define the normalization and
   are retained in each report; they are not inferred engineering requirements.
4. Evaluate. Review signed/forward/reverse mass flow, pressure and momentum
   terms, area means, kinetic power and dimensionless section coefficients.
5. Add a snapshot to Comparisons. Rename/change the next analysis, evaluate and
   add it separately. Select a baseline and quantity. Different plane/reference
   definitions produce a visible warning; a zero baseline has no percentage.
6. Export the current report as HTML, CSV or JSON. Import exported JSON reports
   to compare other designs or mesh revisions. Export the comparison as CSV.
7. Save the workspace to the owning editable flow project to retain definitions
   and up to 64 comparison snapshots through the normal project save operation.
   Existing imported dictionaries and legacy cavity configurations remain
   protected; their reports can still be exported independently.

Changing inputs invalidates the current evaluation and disables its export.
Previous comparison snapshots retain their values and definitions. Saving
analysis metadata does not alter the prepared-case fingerprint.

## Definitions and scope

With chosen unit normal n, normal velocity Un = U·n and section area dA:

- signed mass flow = integral(rho Un dA), in kg/s;
- forward/reverse mass flows integrate positive/negative flux magnitudes;
- volume flow = integral(Un dA), in m³/s;
- normal momentum flux = integral(rho Un² dA), in N;
- pressure term = integral((p − p_ref) dA), in N;
- normal section resultant = momentum flux + pressure term;
- signed kinetic power = integral(0.5 rho |U|² Un dA), in W;
- normal section coefficient = resultant / (0.5 rho_ref V_ref² A_ref);
- mean Cp = (area-mean p − p_ref) / (0.5 rho_ref V_ref²).

The section resultant is a control-surface quantity. It does not independently
provide full engine thrust, aircraft lift/drag, skin friction or wall moments.
Use all relevant control surfaces and terms for a complete momentum balance.
See NASA Glenn's [thrust equation](https://www1.grc.nasa.gov/beginners-guide-to-aeronautics/thrust-force/)
and [dynamic pressure](https://www1.grc.nasa.gov/beginners-guide-to-aeronautics/dynamic-pressure/)
for the physical definitions; these references do not validate this implementation.

Reports operate on original three-dimensional volume blocks, including all
nonempty fluid-volume leaves in a multiblock result. Display cuts, camera pose
and the decorative 360° reconstruction do not change integration inputs.
Surface patches are excluded. A known axisymmetric X wedge can explicitly use
nominal full-annulus scaling 360/wedge-angle (up to five degrees); this scales
extensive totals, not averages. It retains the geometric approximation of the
faceted wedge and is not an exact reconstruction of a curved annular surface.
The report records the angle, scale and reference area separately.

Stored cell fields remain piecewise constant. Point fields are interpolated on
cut triangles and integrated with seven-point, degree-five triangle quadrature.
This integrates linear-field products without replacing products by products of
means. Forward/reverse splitting within a triangle that changes flow direction
is a quadrature approximation. Ghost triangles are excluded. Identical
coincident triangles are counted once; conflicting coincident data require an
offset plane. Missing/incomplete fields, non-finite values and non-positive
densities reject the evaluation rather than publishing a partial integral.

## Traceability

JSON/HTML/CSV reports retain algorithm identifier, UTC time, source project and
result path, available source-manifest digest and configuration snapshot, all
reference inputs and resulting quantities. Project settings are explicitly
labelled as captured at analysis time: they are not asserted to be the setup
that produced an older loaded checkpoint. Matching producing-run settings still
requires case/run records. The sample SHA-256 hashes the section
definition, accepted triangle geometry and quadrature field values; it is not a
hash of every field elsewhere in the case. The report SHA-256 covers its
canonical JSON payload. Import verifies schema and digest, with a 4 MiB file
limit. A digest detects changes; it is not a signature, approval or proof that
an imported report came from a trusted solver.

The tool does not promote stored fields to a converged or physically validated
solution. Existing solver assessments, mesh independence and model validation
remain separate evidence. No solver, turbulence model or CFD convergence policy
is changed by this feature.

## Verification

Analytical tests cover constant flow/pressure, normal reversal, variable linear
point fields with exact polynomial integrals, pressure conventions, nominal
wedge scaling, multiple fluid blocks, coincident/ghost data and rejection of
invalid inputs. Report tests cover tampering, HTML escaping, CSV output,
round-tripping, comparison definitions and UI invalidation. A workspace
regression saves/reopens comparisons and checks prepared-physics fingerprints.
Native appearance review covers all three tabs in both themes using a saved CFD
result. These are software/analytical post-processing checks, not CFD validation.
