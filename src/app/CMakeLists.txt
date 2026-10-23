# vortia_app — the Vortia desktop application.

add_executable(vortia
    main.cpp
    "${PROJECT_SOURCE_DIR}/resources/vortia.qrc")
set_target_properties(vortia PROPERTIES
    AUTOMOC ON AUTOUIC ON AUTORCC ON)

target_link_libraries(vortia
    PRIVATE
        vortia::ui
        vortia::visualization
        vortia_warnings)

# Translation generation is REQUIRED for a UI build. Previously an optional
# find_program silently skipped lrelease and produced an English-only binary.
find_package(Qt6 REQUIRED COMPONENTS LinguistTools)

set(VORTIA_TS_FILE "${PROJECT_SOURCE_DIR}/resources/translations/vortia_es.ts")
set(VORTIA_QM_FILE "${CMAKE_BINARY_DIR}/vortia_es.qm")
add_custom_command(
    OUTPUT "${VORTIA_QM_FILE}"
    COMMAND $<TARGET_FILE:Qt6::lrelease> "${VORTIA_TS_FILE}" -qm "${VORTIA_QM_FILE}"
    DEPENDS "${VORTIA_TS_FILE}"
    COMMENT "Compiling Vortia Spanish translations"
    VERBATIM)
add_custom_target(vortia_translations ALL DEPENDS "${VORTIA_QM_FILE}")
add_dependencies(vortia vortia_translations)

set(VORTIA_TRANSLATIONS_QRC "${CMAKE_CURRENT_BINARY_DIR}/vortia_lang.qrc")
file(WRITE "${VORTIA_TRANSLATIONS_QRC}"
    "<RCC><qresource prefix=\"/translations\"><file alias=\"vortia_es.qm\">${VORTIA_QM_FILE}</file></qresource></RCC>")
target_sources(vortia PRIVATE "${VORTIA_TRANSLATIONS_QRC}")

target_compile_definitions(vortia PRIVATE
    VORTIA_VERSION="${PROJECT_VERSION}")

vtk_module_autoinit(TARGETS vortia MODULES ${VTK_LIBRARIES})

# An optional Finder-launchable local macOS bundle. Keep the existing
# build/bin/vortia executable unchanged for development and CI.
if(APPLE)
    add_custom_target(vortia_macos_bundle
        COMMAND /bin/bash
            "${PROJECT_SOURCE_DIR}/packaging/macos/create_bundle.sh"
            "$<TARGET_FILE:vortia>"
            "${PROJECT_SOURCE_DIR}/resources/icons/vortia.png"
            "${CMAKE_BINARY_DIR}/Vortia.app"
            "${PROJECT_SOURCE_DIR}/packaging/macos/Info.plist"
        DEPENDS vortia
        COMMENT "Creating Vortia.app with the Vortia Dock/Finder icon"
        VERBATIM)
endif()
