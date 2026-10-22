# VortiaSanitizers.cmake
# Option VORTIA_ENABLE_SANITIZERS: build with ASan+UBSan (Clang/GCC, non-Windows).

option(VORTIA_ENABLE_SANITIZERS "Enable AddressSanitizer and UndefinedBehaviorSanitizer" OFF)

if(VORTIA_ENABLE_SANITIZERS AND NOT MSVC)
    add_compile_options(-fsanitize=address,undefined -fno-omit-frame-pointer)
    add_link_options(-fsanitize=address,undefined)
    message(STATUS "Vortia: ASan+UBSan enabled")
endif()
