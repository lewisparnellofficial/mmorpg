cmake_minimum_required(VERSION 3.21)

project(mmorpg-editor-qt LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTORCC ON)
set(CMAKE_AUTOUIC ON)

find_package(Qt6 6.8 REQUIRED COMPONENTS Gui Quick Quick3D QuickControls2)

qt_standard_project_setup(REQUIRES 6.8)

qt_add_executable(mmorpg-editor-qt
    main.cpp
)

qt_add_qml_module(mmorpg-editor-qt
    URI Mmorpg.Editor
    VERSION 1.0
    QML_FILES Main.qml
)

target_link_libraries(mmorpg-editor-qt
    PRIVATE
        Qt6::Gui
        Qt6::Quick
        Qt6::Quick3D
        Qt6::QuickControls2
)
