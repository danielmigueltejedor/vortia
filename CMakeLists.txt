cmake_minimum_required(VERSION 3.24)

project(Vortia
    VERSION 0.1.0
    DESCRIPTION "Open professional CFD desktop application powered by OpenFOAM"
    HOMEPAGE_URL "https://github.com/vortia/vortia"
    LANGUAGES CXX)

# --------------------------------------------------------------------------
# Global settings
# --------------------------------------------------------------------------
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

set(CMAKE_AUTOMOC OFF)
set(CMAKE_AUTOUIC OFF)
set(CMAKE_AUTORCC OFF)

if(NOT CMAKE_BUILD_TYPE AND NOT CMAKE_CONFIGURATION_TYPES)
    set(CMAKE_BUILD_TYPE Release CACHE STRING "Build type" FORCE)
endif()

option(VORTIA_BUILD_TESTS "Build Vortia tests" ON)
option(VORTIA_BUILD_UI   "Build Qt/VTK user interface" ON)
option(VORTIA_WARNINGS_AS_ERRORS "Treat compiler warnings as errors" OFF)

list(APPEND CMAKE_MODULE_PATH "${CMAKE_CURRENT_SOURCE_DIR}/cmake")
include(VortiaCompilerWarnings)
include(VortiaSanitizers)

# Homebrew prefixes (Apple Silicon and Intel)
if(APPLE)
    list(APPEND CMAKE_PREFIX_PATH
        /opt/homebrew/opt/qt
        /opt/homebrew/opt/qt6
        /opt/homebrew/opt/vtk
        /opt/homebrew/opt/opencascade
        /opt/homebrew/opt/nlohmann-json
        /opt/homebrew
        /usr/local/opt/qt
        /usr/local/opt/vtk
        /usr/local)
endif()

# nlohmann_json is a core dependency (project persistence).
find_package(nlohmann_json 3.11 REQUIRED)

# HDF5 is required for Fluent CFF (.cas.h5 / .dat.h5 / .msh.h5) interop.
find_package(HDF5 REQUIRED)

set(CMAKE_RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/bin")

# --------------------------------------------------------------------------
# Core libraries (no GUI dependencies)
# --------------------------------------------------------------------------
add_subdirectory(src/core)
add_subdirectory(src/geometry)
add_subdirectory(src/mesh)
add_subdirectory(src/physics)
add_subdirectory(src/solver)

# --------------------------------------------------------------------------
# UI (Qt 6 + VTK). Gracefully skipped when dependencies are missing so that
# core libraries and tests still build in minimal environments (e.g. CI).
# --------------------------------------------------------------------------
if(VORTIA_BUILD_UI)
    find_package(Qt6 QUIET COMPONENTS Widgets OpenGLWidgets)
    find_package(VTK QUIET COMPONENTS
        CommonCore
        CommonDataModel
        FiltersCore
        FiltersGeometry
        FiltersModeling
        FiltersSources
        IOGeometry
        IOLegacy
        IOXML
        InteractionStyle
        InteractionWidgets
        RenderingCore
        RenderingOpenGL2
        RenderingAnnotation
        GUISupportQt)

    if(Qt6_FOUND AND VTK_FOUND)
        message(STATUS "Vortia: Qt ${Qt6_VERSION} and VTK ${VTK_VERSION} found — building UI")
        add_subdirectory(src/visualization)
        add_subdirectory(src/interop)
        add_subdirectory(src/ui)
        add_subdirectory(src/app)
    else()
        message(WARNING
            "Vortia: Qt6 and/or VTK with GUISupportQt not found. "
            "Building core libraries and tests only (UI disabled).")
    endif()
endif()

# --------------------------------------------------------------------------
# Tests
# --------------------------------------------------------------------------
if(VORTIA_BUILD_TESTS)
    enable_testing()
    add_subdirectory(tests)
endif()

# --------------------------------------------------------------------------
# Summary
# --------------------------------------------------------------------------
message(STATUS "")
message(STATUS "Vortia ${PROJECT_VERSION} configuration summary")
message(STATUS "  Build type        : ${CMAKE_BUILD_TYPE}")
message(STATUS "  Tests             : ${VORTIA_BUILD_TESTS}")
message(STATUS "  UI (Qt6+VTK)      : ${VORTIA_BUILD_UI}")
message(STATUS "")
