cmake_minimum_required(VERSION 3.21)
project(nekowall-gui LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Qt 6 con QML. Si falta algo, explicar qué paquete de Void instalar (en lugar del error genérico de CMake).
find_package(Qt6 6.5 QUIET COMPONENTS Core Gui Widgets)
if(NOT Qt6_FOUND)
    message(FATAL_ERROR
        "\nNo se encontraron las bibliotecas de desarrollo de Qt 6.\n"
        "Instálalas con:   sudo xbps-install -S qt6-base-devel qt6-declarative-devel\n"
        "(en neko-void puede ser «doas» en lugar de «sudo»)\n")
endif()
find_package(Qt6 6.5 QUIET COMPONENTS Qml Quick QuickControls2)
if(NOT Qt6Qml_FOUND OR NOT Qt6Quick_FOUND OR NOT Qt6QuickControls2_FOUND)
    message(FATAL_ERROR
        "\nFalta Qt Quick / QML (el paquete de desarrollo de QML de Qt 6).\n"
        "Instálalo con:   sudo xbps-install -S qt6-declarative-devel\n"
        "(en neko-void puede ser «doas» en lugar de «sudo»)\n")
endif()
# Red (Workshop: búsquedas y descargas de Wallhaven). Viene con qt6-base-devel.
find_package(Qt6 6.5 REQUIRED COMPONENTS Network)
qt_standard_project_setup(REQUIRES 6.5)

qt_add_executable(nekowall-gui
    src/main.cpp
    src/Bridge.cpp
    src/Bridge.h
    src/Workshop.cpp
    src/Workshop.h
)

qt_add_qml_module(nekowall-gui
    URI NekoWall
    VERSION 1.0
    QML_FILES
        qml/Hud.qml
        qml/HudWayland.qml
        qml/HudCard.qml
        qml/Skeuo.qml
        qml/Main.qml
        qml/LibraryPage.qml
        qml/SettingsPage.qml
        qml/WallpaperTile.qml
        qml/FilterChip.qml
        qml/WorkshopPage.qml
)

# En desarrollo, la GUI busca la CLI en ../target/release/nekowall si no está junto al ejecutable.
target_compile_definitions(nekowall-gui PRIVATE
    NEKOWALL_DEV_CLI="${CMAKE_CURRENT_SOURCE_DIR}/../target/release/nekowall")

# Desenfoque de KWin detrás del HUD (opcional: sin KF6WindowSystem el HUD es oscuro sin cristal).
find_package(KF6WindowSystem QUIET)
if(KF6WindowSystem_FOUND)
    target_link_libraries(nekowall-gui PRIVATE KF6::WindowSystem)
    target_compile_definitions(nekowall-gui PRIVATE HAVE_KWINDOWEFFECTS)
endif()

target_link_libraries(nekowall-gui PRIVATE Qt6::Core Qt6::Gui Qt6::Widgets Qt6::Qml Qt6::Quick Qt6::QuickControls2 Qt6::Network)

install(TARGETS nekowall-gui RUNTIME DESTINATION bin)
