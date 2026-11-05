# vortia_verification — analytic profiles, reference tables, error metrics.
# Header-only for Milestone A; keep free of Qt/VTK so UI-off CI can link it.

add_library(vortia_verification INTERFACE)
add_library(vortia::verification ALIAS vortia_verification)

target_include_directories(vortia_verification
    INTERFACE
        ${CMAKE_CURRENT_SOURCE_DIR}/include)

target_link_libraries(vortia_verification
    INTERFACE
        vortia::physics)

target_compile_features(vortia_verification INTERFACE cxx_std_20)
