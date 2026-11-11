include("${EMELINO_CONFIG}")
idf_component_register(SRCS ${EMELINO_SOURCES} INCLUDE_DIRS ${EMELINO_INCLUDES}
    REQUIRES freertos esp_timer esp_driver_gpio esp_driver_uart esp_driver_spi esp_driver_i2c)
target_compile_options(${COMPONENT_LIB} PRIVATE "-${EMELINO_OPTIMIZATION}" -g3 -Wall -Wextra -Werror -fstack-usage)
