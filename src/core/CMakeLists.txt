# vortia_core — pure C++20 foundations: errors, units, logging, processes, projects.

add_library(vortia_core STATIC
    src/Process.cpp
    src/Project.cpp
    src/Version.cpp)

add_library(vortia::core ALIAS vortia_core)

target_include_directories(vortia_core
    PUBLIC
        $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
        $<INSTALL_INTERFACE:include>)

target_link_libraries(vortia_core
    PUBLIC nlohmann_json::nlohmann_json
    PRIVATE vortia_warnings)

target_compile_features(vortia_core PUBLIC cxx_std_20)
