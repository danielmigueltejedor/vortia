# Vortia test suites (GoogleTest).

find_package(GTest REQUIRED)
include(GoogleTest)

# Shared test helpers (private temp dirs, etc.).
set(VORTIA_TEST_SUPPORT_INCLUDE "${CMAKE_CURRENT_SOURCE_DIR}/support")

add_executable(test_core
    unit/test_units.cpp
    unit/test_expected.cpp
    unit/test_version.cpp
    unit/test_host_environment.cpp
    unit/test_private_temp_dir.cpp
    unit/test_process.cpp
    unit/test_project.cpp)
target_include_directories(test_core PRIVATE ${VORTIA_TEST_SUPPORT_INCLUDE})
target_link_libraries(test_core PRIVATE vortia::core GTest::gtest_main vortia_warnings)
gtest_discover_tests(test_core)

add_executable(test_geometry
    unit/test_triangle_mesh.cpp
    unit/test_stl_reader.cpp
    unit/test_box_domain.cpp
    unit/test_sketch_2d.cpp
    unit/test_sketch_snapping.cpp
    unit/test_sketch_arcs.cpp
    unit/test_sketch_templates.cpp
    unit/test_sketch_file.cpp)
target_include_directories(test_geometry PRIVATE ${VORTIA_TEST_SUPPORT_INCLUDE})
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

add_executable(test_verification
    unit/test_verification.cpp)
target_include_directories(test_verification PRIVATE ${VORTIA_TEST_SUPPORT_INCLUDE})
target_link_libraries(test_verification PRIVATE
    vortia::verification
    vortia::solver
    GTest::gtest_main
    vortia_warnings)
target_compile_definitions(test_verification PRIVATE
    VORTIA_BENCHMARK_ROOT="${PROJECT_SOURCE_DIR}/benchmarks")
gtest_discover_tests(test_verification)

# Always-on: CLI can materialize case directories (no OpenFOAM run).
add_test(NAME benchmark_write_poiseuille_smoke
    COMMAND $<TARGET_FILE:benchmark_write_case> poiseuille
            "${CMAKE_BINARY_DIR}/benchmark-smoke/poiseuille")
add_test(NAME benchmark_write_blasius_smoke
    COMMAND $<TARGET_FILE:benchmark_write_case> blasius
            "${CMAKE_BINARY_DIR}/benchmark-smoke/blasius")

# Native OpenFOAM verification: exit 2 = tools missing → CTest SKIP.
add_test(NAME benchmark_poiseuille_openfoam
    COMMAND bash "${PROJECT_SOURCE_DIR}/benchmarks/poiseuille-laminar-2d/scripts/run_openfoam.sh"
            "${CMAKE_BINARY_DIR}/benchmark-native/poiseuille")
set_tests_properties(benchmark_poiseuille_openfoam PROPERTIES
    ENVIRONMENT "VORTIA_BENCHMARK_WRITE_CASE=$<TARGET_FILE:benchmark_write_case>"
    SKIP_RETURN_CODE 2
    TIMEOUT 600)

add_test(NAME benchmark_ghia_openfoam
    COMMAND bash "${PROJECT_SOURCE_DIR}/benchmarks/lid-driven-cavity-ghia-re100/scripts/run_openfoam.sh"
            "${CMAKE_BINARY_DIR}/benchmark-native/ghia")
set_tests_properties(benchmark_ghia_openfoam PROPERTIES
    ENVIRONMENT "VORTIA_BENCHMARK_WRITE_CASE=$<TARGET_FILE:benchmark_write_case>"
    SKIP_RETURN_CODE 2
    TIMEOUT 1200)

add_test(NAME benchmark_turbulent_plate_openfoam
    COMMAND bash "${PROJECT_SOURCE_DIR}/benchmarks/turbulent-flat-plate-sst/scripts/run_openfoam.sh"
            "${CMAKE_BINARY_DIR}/benchmark-native/turbulent-plate")
set_tests_properties(benchmark_turbulent_plate_openfoam PROPERTIES
    ENVIRONMENT "VORTIA_BENCHMARK_WRITE_CASE=$<TARGET_FILE:benchmark_write_case>"
    SKIP_RETURN_CODE 2
    TIMEOUT 2400)

add_test(NAME benchmark_write_turbulent_plate_smoke
    COMMAND $<TARGET_FILE:benchmark_write_case> turbulent-plate
            "${CMAKE_BINARY_DIR}/benchmark-smoke/turbulent-plate")

add_executable(test_solver
    unit/test_installation.cpp
    unit/test_detector.cpp
    unit/test_engine_self_test.cpp
    unit/test_forces.cpp
    unit/test_turbulence.cpp
    unit/test_case_writer.cpp
    unit/test_log_parser.cpp
    unit/test_convergence_policy.cpp)
target_sources(test_solver PRIVATE unit/test_mass_balance.cpp)
target_include_directories(test_solver PRIVATE ${VORTIA_TEST_SUPPORT_INCLUDE})
target_link_libraries(test_solver PRIVATE vortia::solver vortia::verification GTest::gtest_main vortia_warnings)
gtest_discover_tests(test_solver)

# Integration: CaseRunner orchestration against a FAKE backend (shell-script
# doubles). This tests process orchestration only — it is NOT CFD validation
# (see docs/VALIDATION.md).
add_executable(test_integration
    integration/test_case_runner_fake_backend.cpp)
target_include_directories(test_integration PRIVATE ${VORTIA_TEST_SUPPORT_INCLUDE})
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

# AppUpdater pure helpers (SHA-256 digest + macOS publisher policy). Needs Qt Core only.
find_package(Qt6 QUIET COMPONENTS Core)
if(Qt6_FOUND)
    add_executable(test_app_updater unit/test_app_updater.cpp unit/test_update_helper.cpp)
    target_include_directories(test_app_updater PRIVATE
        ${PROJECT_SOURCE_DIR}/src/ui/include
        ${PROJECT_SOURCE_DIR}/src/ui/src)
    target_link_libraries(test_app_updater PRIVATE Qt6::Core GTest::gtest_main vortia_warnings)
    gtest_discover_tests(test_app_updater)
endif()

if(TARGET vortia_engine)
    add_executable(test_engine unit/test_engine.cpp)
    target_include_directories(test_engine PRIVATE ${VORTIA_TEST_SUPPORT_INCLUDE})
    target_link_libraries(test_engine PRIVATE vortia::engine GTest::gtest_main vortia_warnings)
    vtk_module_autoinit(TARGETS test_engine MODULES ${VTK_LIBRARIES})
    gtest_discover_tests(test_engine)
endif()

# CI regression: ensures that lrelease ran and the built app embeds a readable
# Spanish catalog in the same resource path used at runtime.
if(TARGET vortia)
    add_test(NAME vortia_spanish_catalog
             COMMAND $<TARGET_FILE:vortia> --verify-spanish)
    add_executable(test_workspace unit/test_workspace.cpp unit/test_engineering.cpp unit/test_run_diagnostics.cpp unit/test_ndl.cpp
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
    add_test(NAME ndl_icons
        COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/tools/sync_ndl_icons.py --check)
    add_test(NAME general_sketch_flow_reference
        COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/resources/flow/test_prepare.py)
    add_test(NAME nozzle_video_reference
        COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/examples/nozzle-video-2026/test_prepare.py)
    add_test(NAME compare_profiles
        COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/tests/native/test_compare_profiles.py)
    add_test(NAME gci_tool
        COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/tests/native/test_gci.py)
endif()

# Small standalone exporter for actual OpenFOAM blockMesh/checkMesh smoke tests.
add_executable(nozzle_mesh_export_smoke integration/nozzle_mesh_export_smoke.cpp)
target_link_libraries(nozzle_mesh_export_smoke PRIVATE vortia::mesh vortia_warnings)
