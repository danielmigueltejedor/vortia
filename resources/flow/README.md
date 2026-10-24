# General flow from a CAD sketch

The normal Model tools configure the active editable sketch, mesh sizes, ideal-gas material, laminar/Spalart–Allmaras model, per-segment regions, total-pressure inlet, pressure outlet, open reservoir, adiabatic no-slip wall and initialization/calculation controls. There is no tutorial-specific command in the application. Lines and exact circular arcs are meshed by a separate Gmsh 4.15.2 process and converted with gmshToFoam. One closed fluid cross-section at radius Y >= 0 is supported; X is the rotation axis. Multiple bodies, holes, Boolean CAD and general 3D meshing are not implemented.

The source project persists its sketch and flow-setup.json. Mesh changes create new calculation folders and retain earlier results. Unchanged inputs can resume the active calculation. Project paths containing spaces use managed calculation storage under ~/Library/Vortia/cases to keep OpenFOAM build paths valid. Imported cases remain protected; edit the source project instead.

The native compressible backend remains experimental. It uses the existing independent GPL OpenFOAM v2606 helper, enforces no swirl, transports energy and viscous work, and supports a single constant-property ideal gas. It is not an ANSYS Fluent clone or a validated compressible nozzle workflow. Check field admissibility, boundary mass balance, field evolution and mesh dependence. No CFD result is synthesized by the preparer.

Python 3 and Gmsh are required for preparation. The macOS app includes the Gmsh Python API/shared library and its complete license; OpenFOAM v2606 and its build toolchain are external requirements. The standard-library tests cover input validation and field propagation. A native 1618-cell arbitrary duct mesh passed checkMesh and 100 cold-start steps retained finite positive p/T/rho and zero internal Uz. That is an initial software/backend check, not CFD validation.

Gmsh source and documentation: https://gmsh.info/ and https://gitlab.onelab.info/gmsh/gmsh . The application invokes its GPL runtime as a separate process. The OpenFOAM helper license/source are retained in each generated case.
