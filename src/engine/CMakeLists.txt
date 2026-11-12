# vortia-engine — the case pipeline behind newline-delimited JSON, for Vortia Native.
# It reads results through VTK and finds the packaged OpenFOAM with QtCore; it draws nothing.

add_library(vortia_engine STATIC Engine.cpp Engine.hpp
    # Compressible flow: the same QtCore code the desktop app runs, without its dialogs.
    ${PROJECT_SOURCE_DIR}/src/ui/src/BoundaryMapping.cpp
    ${PROJECT_SOURCE_DIR}/src/ui/src/FlowConfiguration.cpp
    ${PROJECT_SOURCE_DIR}/src/ui/src/FlowCase.cpp)
add_library(vortia::engine ALIAS vortia_engine)
target_include_directories(vortia_engine
    PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}
    # Header-only helpers shared with the desktop app (results, the packaged OpenFOAM).
    PUBLIC ${PROJECT_SOURCE_DIR}/src/ui/include)
target_link_libraries(vortia_engine
    PUBLIC
        vortia::core
        vortia::physics
        vortia::solver
        vortia::geometry
        vortia::visualization
        nlohmann_json::nlohmann_json
        Qt6::Core
    PRIVATE vortia_warnings)
target_compile_definitions(vortia_engine PRIVATE VORTIA_VERSION="${PROJECT_VERSION}")
target_compile_features(vortia_engine PUBLIC cxx_std_20)

# The mesher scripts and the vortiaCentralFoam sources travel as Qt resources, as in the app.
add_executable(vortia-engine main.cpp "${PROJECT_SOURCE_DIR}/resources/vortia.qrc")
set_target_properties(vortia-engine PROPERTIES AUTORCC ON)
target_link_libraries(vortia-engine PRIVATE vortia::engine vortia_warnings)
vtk_module_autoinit(TARGETS vortia_engine vortia-engine MODULES ${VTK_LIBRARIES})
