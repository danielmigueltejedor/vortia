# Experimental axisymmetric nozzle wedge mesh

OpenFOAM's official user guide documents axisymmetric meshes using collapsed
hex vertices, one circumferential cell and a pair of wedge patches.

Vortia now generates this representation for an **unmodified parametric nozzle**:
- shared axial stations, axis vertices collapsed in each hex;
- two wedge patches with angles ±(angle/2), one circumferential cell;
- axis patch with zero area, specified as \`empty\`;
- inlet, outlet and curved polygonal wall named patches;
- editable converging/diverging block segmentation, axial/radial cells,
  radial grading and angle (up to 5 degrees);
- length units in millimetres, OpenFOAM \`convertToMeters 0.001\`.

### Caveats / verification gates

This is a *blockMeshDict exporter*, **not** a complete OpenFOAM case,
not the energy-equation/ideal-gas/SST compressible simulation and not yet
an engineer-validated mesh. Automated tests cover dictionary structure and
station geometry only. They do not execute OpenFOAM.

**Mandatory before use:** export \`system/blockMeshDict\` into a suitable
OpenFOAM case, run \`blockMesh -case <case>\`, then \`checkMesh -case <case>
-allTopology -allGeometry\`. Review 3D wedge-plane alignment, face normals,
non-orthogonality, skewness and collapsed-axis handling. Do not use Vortia's
existing incompressible cavity case for a supersonic nozzle analysis.

Reference:
https://doc.cfd.direct/openfoam/user-guide-v13/blockmesh
https://www.openfoam.com/documentation/user-guide/4-mesh-generation-and-conversion/4.3-mesh-generation-with-the-blockmesh-utility
