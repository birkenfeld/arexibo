cmake_minimum_required(VERSION 3.16)

project(arexibogui LANGUAGES CXX)

set(CMAKE_AUTOUIC ON)
set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTORCC ON)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(QT NAMES Qt6 REQUIRED COMPONENTS Widgets WebEngineWidgets WebChannel)
find_package(Qt${QT_VERSION_MAJOR} REQUIRED COMPONENTS Widgets WebEngineWidgets WebChannel)

add_library(arexibogui STATIC
    view.cpp
    view.h
    lib.cpp
    lib.h
    res.qrc
)

target_link_libraries(arexibogui PRIVATE Qt${QT_VERSION_MAJOR}::Widgets)

target_compile_options(arexibogui PRIVATE -Wall -pedantic)

install(TARGETS arexibogui)
