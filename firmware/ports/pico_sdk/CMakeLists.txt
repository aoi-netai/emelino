cmake_minimum_required(VERSION 3.20)
include("${EMELINO_CONFIG}")
include("${PICO_SDK_PATH}/pico_sdk_init.cmake")
project(emelino_pico C CXX ASM)
set(CMAKE_C_STANDARD 11)
set(CMAKE_CXX_STANDARD 17)

# Refuse SDK's fallback download path. Setup installs this CONFIG package first.
find_package(picotool 2.3.1 EXACT REQUIRED CONFIG NO_CMAKE_FIND_ROOT_PATH)
pico_sdk_init()
file(GENERATE OUTPUT "${EMELINO_OUTPUT}/picotool-path.txt" CONTENT "$<TARGET_FILE:picotool>\n")
file(WRITE "${EMELINO_OUTPUT}/compiler-path.txt" "${CMAKE_C_COMPILER}\n")
add_executable(firmware ${EMELINO_SOURCES})
set_target_properties(firmware PROPERTIES LINKER_LANGUAGE C
    RUNTIME_OUTPUT_DIRECTORY "${EMELINO_OUTPUT}")
target_include_directories(firmware PRIVATE ${EMELINO_INCLUDES})
target_compile_options(firmware PRIVATE "-${EMELINO_OPTIMIZATION}" -g3
    $<$<COMPILE_LANGUAGE:C>:-fstack-usage>)
set_source_files_properties(${EMELINO_SOURCES} PROPERTIES
    COMPILE_OPTIONS "-Wall;-Wextra;-Werror")
target_compile_definitions(firmware PRIVATE
    PICO_STACK_SIZE=${EMELINO_STACK_BYTES}
    PICO_HEAP_SIZE=0 PICO_CXX_DISABLE_ALLOCATION_OVERRIDES=1
    PICO_DEFAULT_UART=0)
target_link_libraries(firmware PRIVATE pico_stdlib hardware_gpio hardware_irq
    hardware_sync hardware_timer ${EMELINO_LIBRARIES})
pico_minimize_runtime(firmware INCLUDE FLOAT DOUBLE)
pico_enable_stdio_usb(firmware 0)
pico_enable_stdio_uart(firmware 0)
pico_add_linker_script_override_path(firmware "${EMELINO_OUTPUT}/linker"
    FILES set_memory_locations.incl)
pico_set_linker_script_var(firmware PICO_FLASH_SIZE_BYTES ${EMELINO_FLASH_BYTES})
target_link_options(firmware PRIVATE ${EMELINO_LINK_ARGS})
pico_add_extra_outputs(firmware)
