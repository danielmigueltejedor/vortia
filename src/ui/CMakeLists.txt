# vortia_ui — Qt Widgets: main window, docks, panels, job control.

add_library(vortia_ui STATIC
    include/vortia/ui/AppUpdater.hpp
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
    src/AppUpdater.cpp
    src/EngineeringReport.cpp
    src/EngineeringWorkbench.cpp
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
    src/FlowCase.cpp
    src/FlowConfiguration.cpp
    src/FieldStatisticsDialog.cpp
    src/FieldTools.cpp
    src/LineProfile.cpp
    src/RunMonitor.cpp
    src/RunDiagnostics.cpp
    src/ProjectExplorerPanel.cpp
    src/PropertiesPanel.cpp
    src/ThemeManager.cpp
    include/vortia/ui/WindowChrome.hpp
    src/WindowChrome.cpp
    include/vortia/ui/GlobalMenu.hpp
    src/GlobalMenu.cpp
    src/SketchEditorDialog.cpp
    include/vortia/ui/WelcomeScreen.hpp
    src/WelcomeScreen.cpp)

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
        Qt6::Network
    PRIVATE vortia_warnings)

set_target_properties(vortia_ui PROPERTIES
    AUTOMOC ON AUTOUIC ON AUTORCC ON)
target_compile_features(vortia_ui PUBLIC cxx_std_20)

# The top-bar menu on Linux goes over D-Bus.
if(UNIX AND NOT APPLE)
    find_package(Qt6 REQUIRED COMPONENTS DBus)
    target_link_libraries(vortia_ui PRIVATE Qt6::DBus)
endif()

if(APPLE)
    enable_language(OBJCXX)
    target_sources(vortia_ui PRIVATE src/MacWindow.mm)
    target_link_libraries(vortia_ui PRIVATE "-framework AppKit")
endif()
