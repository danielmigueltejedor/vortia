# vortia_ui — Qt Widgets: main window, docks, panels, job control.

add_library(vortia_ui STATIC
    include/vortia/ui/ConsolePanel.hpp
    include/vortia/ui/JobController.hpp
    include/vortia/ui/MainWindow.hpp
    include/vortia/ui/RunMonitor.hpp
    include/vortia/ui/ProjectExplorerPanel.hpp
    include/vortia/ui/PropertiesPanel.hpp
    include/vortia/ui/ThemeManager.hpp
    include/vortia/ui/UiIcon.hpp
    include/vortia/ui/FlowSetup.hpp
    include/vortia/ui/SketchEditorDialog.hpp
    src/ApplicationPreferences.cpp
    src/ConsolePanel.cpp
    src/JobController.cpp
    src/MainWindow.cpp
    src/FlowSetup.cpp
    include/vortia/ui/PlanarView.hpp
    include/vortia/ui/MeshRegionsWidget.hpp
    src/PlanarView.cpp
    src/MeshRegionsWidget.cpp
    src/BoundaryPreview.cpp
    src/BoundaryMapping.cpp
    src/FieldStatisticsDialog.cpp
    src/FieldTools.cpp
    src/LineProfile.cpp
    src/RunMonitor.cpp
    src/ProjectExplorerPanel.cpp
    src/PropertiesPanel.cpp
    src/ThemeManager.cpp
    src/SketchEditorDialog.cpp)

add_library(vortia::ui ALIAS vortia_ui)

target_include_directories(vortia_ui
    PUBLIC
        ${CMAKE_CURRENT_SOURCE_DIR}/include)

target_link_libraries(vortia_ui
    PUBLIC
        vortia::visualization
        vortia::interop
        vortia::solver
        vortia::physics
        vortia::mesh
        vortia::geometry
        vortia::core
        Qt6::Widgets
        Qt6::OpenGLWidgets
    PRIVATE vortia_warnings)

set_target_properties(vortia_ui PROPERTIES
    AUTOMOC ON AUTOUIC ON AUTORCC ON)
target_compile_features(vortia_ui PUBLIC cxx_std_20)

if(APPLE)
    enable_language(OBJCXX)
    target_sources(vortia_ui PRIVATE src/MacWindow.mm)
    target_link_libraries(vortia_ui PRIVATE "-framework AppKit")
endif()
