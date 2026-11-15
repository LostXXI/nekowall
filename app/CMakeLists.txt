cmake_minimum_required(VERSION 3.21)
project(nekowall-gui LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(Qt6 6.5 REQUIRED COMPONENTS Core Gui Widgets Qml Quick QuickControls2)
qt_standard_project_setup(REQUIRES 6.5)

qt_add_executable(nekowall-gui
    src/main.cpp
    src/Bridge.cpp
    src/Bridge.h
)

qt_add_qml_module(nekowall-gui
    URI NekoWall
    VERSION 1.0
    QML_FILES
        qml/Hud.qml
        qml/HudCard.qml
        qml/Skeuo.qml
        qml/Main.qml
        qml/LibraryPage.qml
        qml/SettingsPage.qml
        qml/WallpaperTile.qml
        qml/FilterChip.qml
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

target_link_libraries(nekowall-gui PRIVATE Qt6::Core Qt6::Gui Qt6::Widgets Qt6::Qml Qt6::Quick Qt6::QuickControls2)

install(TARGETS nekowall-gui RUNTIME DESTINATION bin)
