# vortia_visualization — VTK scene management and the Qt-embedded viewport.

add_library(vortia_visualization STATIC
    include/vortia/visualization/VtkViewport.hpp
    src/VtkViewport.cpp)

add_library(vortia::visualization ALIAS vortia_visualization)

target_include_directories(vortia_visualization
    PUBLIC
        ${CMAKE_CURRENT_SOURCE_DIR}/include)

target_link_libraries(vortia_visualization
    PUBLIC
        vortia::geometry
        Qt6::Widgets
        Qt6::OpenGLWidgets
        ${VTK_LIBRARIES}
    PRIVATE vortia_warnings)

set_target_properties(vortia_visualization PROPERTIES
    AUTOMOC ON AUTOUIC ON AUTORCC ON)
target_compile_features(vortia_visualization PUBLIC cxx_std_20)

# Required when VTK is built statically; harmless for shared builds.
vtk_module_autoinit(
    TARGETS vortia_visualization
    MODULES ${VTK_LIBRARIES})
