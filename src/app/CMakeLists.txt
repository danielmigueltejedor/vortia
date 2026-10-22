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

# Translation sources (compiled into the binary via the Qt resource system).
# The .ts file is the source of truth; lrelease runs at build time.
find_program(VORTIA_LRELEASE NAMES lrelease lrelease-qt6 Qt6::lrelease)
if(VORTIA_LRELEASE)
    set(VORTIA_TS_FILE "${CMAKE_CURRENT_SOURCE_DIR}/../../resources/translations/vortia_es.ts")
    if(EXISTS "${VORTIA_TS_FILE}")
        add_custom_command(
            OUTPUT "${CMAKE_BINARY_DIR}/vortia_es.qm"
            COMMAND ${VORTIA_LRELEASE} -silent "${VORTIA_TS_FILE}"
                    -qm "${CMAKE_BINARY_DIR}/vortia_es.qm"
            DEPENDS "${VORTIA_TS_FILE}"
            VERBATIM)
        add_custom_target(vortia_translations ALL
            DEPENDS "${CMAKE_BINARY_DIR}/vortia_es.qm")
        add_dependencies(vortia vortia_translations)
    endif()
endif()

# Embed the .qm files as a Qt resource if they exist. Simpler: rely on the
# filesystem resource path at runtime; fall back to no translations.
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
