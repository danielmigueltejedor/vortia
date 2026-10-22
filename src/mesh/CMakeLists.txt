# vortia_mesh — mesh generation parameters and OpenFOAM mesh dictionaries.

add_library(vortia_mesh STATIC
    src/BlockMeshDict.cpp)

add_library(vortia::mesh ALIAS vortia_mesh)

target_include_directories(vortia_mesh
    PUBLIC
        $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
        $<INSTALL_INTERFACE:include>)

target_link_libraries(vortia_mesh
    PUBLIC vortia::geometry
    PRIVATE vortia_warnings)

target_compile_features(vortia_mesh PUBLIC cxx_std_20)
