# vortia_interop — third-party format adapters (ANSYS Fluent first).

add_library(vortia_interop STATIC
    fluent/FluentCffImporter.cpp
    fluent/FluentFormatDetector.cpp
    fluent/FluentMeshReader.cpp
    fluent/FluentToOpenFoam.cpp)

add_library(vortia::interop ALIAS vortia_interop)

target_include_directories(vortia_interop
    PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/include ${HDF5_INCLUDE_DIRS})

target_link_libraries(vortia_interop
    PUBLIC
        vortia::core
        vortia::geometry
        vortia::mesh
        vortia::physics
        vortia::solver
        VTK::IOFLUENTCFF
        VTK::IOGeometry
        VTK::CommonCore
        VTK::CommonDataModel
        ${HDF5_LIBRARIES}
    PRIVATE vortia_warnings)

target_compile_features(vortia_interop PUBLIC cxx_std_20)
set_target_properties(vortia_interop PROPERTIES AUTOMOC ON)