# vortia_physics — materials, boundary conditions, validated case setup.

add_library(vortia_physics STATIC
    src/CaseSetupJson.cpp
    src/Physics.cpp)

add_library(vortia::physics ALIAS vortia_physics)

target_include_directories(vortia_physics
    PUBLIC
        $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
        $<INSTALL_INTERFACE:include>)

target_link_libraries(vortia_physics
    PUBLIC vortia::mesh nlohmann_json::nlohmann_json
    PRIVATE vortia_warnings)

target_compile_features(vortia_physics PUBLIC cxx_std_20)
