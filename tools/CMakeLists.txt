# Small CLI helpers used by scientific benchmarks (no Qt).

add_executable(benchmark_write_case benchmark_write_case.cpp)
target_link_libraries(benchmark_write_case PRIVATE
    vortia::verification
    vortia::solver
    vortia_warnings)
target_compile_features(benchmark_write_case PRIVATE cxx_std_20)

add_executable(force_report force_report.cpp)
target_link_libraries(force_report PRIVATE
    vortia::verification
    vortia::solver
    vortia_warnings)
target_compile_features(force_report PRIVATE cxx_std_20)
