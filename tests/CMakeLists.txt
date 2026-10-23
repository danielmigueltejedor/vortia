# Vortia test suites (GoogleTest).

find_package(GTest REQUIRED)
include(GoogleTest)

add_executable(test_core
    unit/test_units.cpp
    unit/test_expected.cpp
    unit/test_version.cpp
    unit/test_process.cpp
    unit/test_project.cpp)
target_link_libraries(test_core PRIVATE vortia::core GTest::gtest_main vortia_warnings)
gtest_discover_tests(test_core)

add_executable(test_geometry
    unit/test_triangle_mesh.cpp
    unit/test_stl_reader.cpp
    unit/test_box_domain.cpp
    unit/test_sketch_2d.cpp)
target_link_libraries(test_geometry PRIVATE vortia::geometry GTest::gtest_main vortia_warnings)
gtest_discover_tests(test_geometry)

add_executable(test_mesh
    unit/test_block_mesh_dict.cpp)
target_link_libraries(test_mesh PRIVATE vortia::mesh GTest::gtest_main vortia_warnings)
gtest_discover_tests(test_mesh)

add_executable(test_physics
    unit/test_case_setup_json.cpp
    unit/test_physics.cpp)
target_link_libraries(test_physics PRIVATE vortia::physics GTest::gtest_main vortia_warnings)
gtest_discover_tests(test_physics)

add_executable(test_solver
    unit/test_installation.cpp
    unit/test_detector.cpp
    unit/test_case_writer.cpp
    unit/test_log_parser.cpp)
target_link_libraries(test_solver PRIVATE vortia::solver GTest::gtest_main vortia_warnings)
gtest_discover_tests(test_solver)

# Integration: CaseRunner orchestration against a FAKE backend (shell-script
# doubles). This tests process orchestration only — it is NOT CFD validation
# (see docs/VALIDATION.md).
add_executable(test_integration
    integration/test_case_runner_fake_backend.cpp)
target_link_libraries(test_integration PRIVATE vortia::solver GTest::gtest_main vortia_warnings)
gtest_discover_tests(test_integration)

# VTK camera safety regression: no real Qt press, no camera interaction.
# The test uses the interactor style only, so it needs no OpenGL context.
if(TARGET vortia_visualization)
    add_executable(test_vtk_input unit/test_vtk_input.cpp)
    target_link_libraries(test_vtk_input PRIVATE vortia::visualization
                          GTest::gtest_main vortia_warnings)
    gtest_discover_tests(test_vtk_input)
endif()
