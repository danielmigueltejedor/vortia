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
    unit/test_sketch_2d.cpp
    unit/test_sketch_snapping.cpp
    unit/test_sketch_arcs.cpp
    unit/test_sketch_templates.cpp)
target_link_libraries(test_geometry PRIVATE vortia::geometry GTest::gtest_main vortia_warnings)
gtest_discover_tests(test_geometry)

add_executable(test_mesh
    unit/test_block_mesh_dict.cpp
    unit/test_planar_nozzle_block_mesh.cpp
    unit/test_axisymmetric_nozzle_block_mesh.cpp)
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
    unit/test_log_parser.cpp
    unit/test_convergence_policy.cpp)
target_sources(test_solver PRIVATE unit/test_mass_balance.cpp)
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

# CI regression: ensures that lrelease ran and the built app embeds a readable
# Spanish catalog in the same resource path used at runtime.
if(TARGET vortia)
    add_test(NAME vortia_spanish_catalog
             COMMAND $<TARGET_FILE:vortia> --verify-spanish)
    add_executable(test_workspace unit/test_workspace.cpp unit/test_engineering.cpp unit/test_run_diagnostics.cpp
        "${PROJECT_SOURCE_DIR}/resources/vortia.qrc")
    set_target_properties(test_workspace PROPERTIES AUTORCC ON)
    target_link_libraries(test_workspace PRIVATE vortia::ui GTest::gtest vortia_warnings)
    target_compile_definitions(test_workspace PRIVATE VORTIA_TEST_QM="${CMAKE_BINARY_DIR}/vortia_es.qm")
    add_dependencies(test_workspace vortia_translations)
    vtk_module_autoinit(TARGETS test_workspace MODULES ${VTK_LIBRARIES})
    add_test(NAME vortia_workspace COMMAND test_workspace)
    set_tests_properties(vortia_workspace PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 60)
endif()

# The tutorial preparer uses only the Python standard library.
find_package(Python3 COMPONENTS Interpreter QUIET)
if(Python3_Interpreter_FOUND)
    add_test(NAME general_sketch_flow_reference
        COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/resources/flow/test_prepare.py)
    add_test(NAME nozzle_video_reference
        COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/examples/nozzle-video-2026/test_prepare.py)
endif()

# Small standalone exporter for actual OpenFOAM blockMesh/checkMesh smoke tests.
add_executable(nozzle_mesh_export_smoke integration/nozzle_mesh_export_smoke.cpp)
target_link_libraries(nozzle_mesh_export_smoke PRIVATE vortia::mesh vortia_warnings)
