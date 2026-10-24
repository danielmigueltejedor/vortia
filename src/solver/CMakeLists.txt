# vortia_solver — CFD engine adapters (OpenFOAM first).

add_library(vortia_solver STATIC
    src/openfoam/CaseRunner.cpp
    src/openfoam/CaseWriter.cpp
    src/openfoam/Detector.cpp
    src/openfoam/Installation.cpp
    src/openfoam/LogParser.cpp)
target_sources(vortia_solver PRIVATE src/openfoam/MassBalance.cpp)

add_library(vortia::solver ALIAS vortia_solver)

target_include_directories(vortia_solver
    PUBLIC
        $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
        $<INSTALL_INTERFACE:include>)

target_link_libraries(vortia_solver
    PUBLIC vortia::physics
    PRIVATE vortia_warnings)

target_compile_features(vortia_solver PUBLIC cxx_std_20)
