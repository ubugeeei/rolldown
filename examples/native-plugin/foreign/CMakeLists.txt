cmake_minimum_required(VERSION 3.20)

project(rolldown_native_plugin_foreign_examples LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_C_EXTENSIONS OFF)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

get_filename_component(REPOSITORY_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../.." ABSOLUTE)
set(ABI_INCLUDE_DIRECTORY "${REPOSITORY_ROOT}/crates/rolldown_plugin_native_abi")
set(ARTIFACT_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}/../artifacts")

file(MAKE_DIRECTORY "${ARTIFACT_DIRECTORY}")

function(configure_native_plugin target)
  target_include_directories(${target} PRIVATE "${ABI_INCLUDE_DIRECTORY}")
  set_target_properties(
    ${target}
    PROPERTIES
      LIBRARY_OUTPUT_DIRECTORY "${ARTIFACT_DIRECTORY}"
      RUNTIME_OUTPUT_DIRECTORY "${ARTIFACT_DIRECTORY}"
      ARCHIVE_OUTPUT_DIRECTORY "${ARTIFACT_DIRECTORY}"
  )

  foreach(configuration DEBUG RELEASE RELWITHDEBINFO MINSIZEREL)
    set_target_properties(
      ${target}
      PROPERTIES
        LIBRARY_OUTPUT_DIRECTORY_${configuration} "${ARTIFACT_DIRECTORY}"
        RUNTIME_OUTPUT_DIRECTORY_${configuration} "${ARTIFACT_DIRECTORY}"
        ARCHIVE_OUTPUT_DIRECTORY_${configuration} "${ARTIFACT_DIRECTORY}"
    )
  endforeach()
endfunction()

add_library(native_plugin_c SHARED c/plugin.c)
configure_native_plugin(native_plugin_c)

add_library(native_plugin_cpp SHARED cpp/plugin.cpp)
configure_native_plugin(native_plugin_cpp)

find_program(GO_EXECUTABLE go REQUIRED)
find_program(ZIG_EXECUTABLE zig REQUIRED)

if(WIN32)
  # cgo requires a GCC-compatible compiler, while the Windows CI generator uses
  # MSVC for the standalone C and C++ examples.
  set(GO_C_COMPILER "${ZIG_EXECUTABLE} cc")
else()
  set(GO_C_COMPILER "${CMAKE_C_COMPILER}")
endif()

set(GO_LIBRARY
    "${ARTIFACT_DIRECTORY}/${CMAKE_SHARED_LIBRARY_PREFIX}native_plugin_go${CMAKE_SHARED_LIBRARY_SUFFIX}")
add_custom_command(
  OUTPUT "${GO_LIBRARY}"
  COMMAND
    "${CMAKE_COMMAND}" -E env
    "CC=${GO_C_COMPILER}"
    "CGO_CFLAGS=-I${ABI_INCLUDE_DIRECTORY}"
    "CGO_ENABLED=1"
    "${GO_EXECUTABLE}" build -buildmode=c-shared -o "${GO_LIBRARY}" .
  WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}/go"
  DEPENDS
    "${ABI_INCLUDE_DIRECTORY}/rolldown_plugin.h"
    go/bridge.c
    go/bridge.h
    go/go.mod
    go/main.go
  VERBATIM
)
add_custom_target(native_plugin_go ALL DEPENDS "${GO_LIBRARY}")

set(ZIG_LIBRARY
    "${ARTIFACT_DIRECTORY}/${CMAKE_SHARED_LIBRARY_PREFIX}native_plugin_zig${CMAKE_SHARED_LIBRARY_SUFFIX}")
add_custom_command(
  OUTPUT "${ZIG_LIBRARY}"
  COMMAND
    "${ZIG_EXECUTABLE}" build-lib
    -dynamic
    -O ReleaseSafe
    -I "${ABI_INCLUDE_DIRECTORY}"
    "-femit-bin=${ZIG_LIBRARY}"
    zig/plugin.zig
  WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
  DEPENDS
    "${ABI_INCLUDE_DIRECTORY}/rolldown_plugin.h"
    zig/plugin.zig
  VERBATIM
)
add_custom_target(native_plugin_zig ALL DEPENDS "${ZIG_LIBRARY}")
