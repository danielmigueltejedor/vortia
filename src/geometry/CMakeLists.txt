# vortia_geometry — triangle meshes, STL import, computational domains.

add_library(vortia_geometry STATIC
    src/BoxDomain.cpp
    src/StlReader.cpp
    src/TriangleMesh.cpp)

add_library(vortia::geometry ALIAS vortia_geometry)

target_include_directories(vortia_geometry
    PUBLIC
        $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
        $<INSTALL_INTERFACE:include>)

target_link_libraries(vortia_geometry
    PUBLIC vortia::core
    PRIVATE vortia_warnings)

target_compile_features(vortia_geometry PUBLIC cxx_std_20)
