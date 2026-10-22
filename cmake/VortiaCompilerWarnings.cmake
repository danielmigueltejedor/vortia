# VortiaCompilerWarnings.cmake
# Provides the interface target `vortia_warnings` with strict warnings.

add_library(vortia_warnings INTERFACE)

if(MSVC)
    target_compile_options(vortia_warnings INTERFACE /W4 /permissive- /utf-8)
    if(VORTIA_WARNINGS_AS_ERRORS)
        target_compile_options(vortia_warnings INTERFACE /WX)
    endif()
else()
    target_compile_options(vortia_warnings INTERFACE
        -Wall
        -Wextra
        -Wpedantic
        -Wshadow
        -Wconversion
        -Wsign-conversion
        -Wnon-virtual-dtor
        -Wold-style-cast
        -Woverloaded-virtual
        -Wnull-dereference
        -Wdouble-promotion
        -Wformat=2)
    if(VORTIA_WARNINGS_AS_ERRORS)
        target_compile_options(vortia_warnings INTERFACE -Werror)
    endif()
endif()
