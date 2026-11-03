/* SPDX-License-Identifier: Apache-2.0 */
#ifndef EMELINO_STM32F4_HAL_CONF_H
#define EMELINO_STM32F4_HAL_CONF_H

/* The BSP supplies fixed reset clocks and a tick only when serial drivers are linked. */
#define HAL_MODULE_ENABLED
#define HAL_GPIO_MODULE_ENABLED
#define HAL_RCC_MODULE_ENABLED
#define HAL_DMA_MODULE_ENABLED
#define HAL_UART_MODULE_ENABLED
#define HAL_SPI_MODULE_ENABLED
#define HAL_I2C_MODULE_ENABLED

#include "stm32f4xx_hal_rcc.h"
#include "stm32f4xx_hal_gpio.h"
#include "stm32f4xx_hal_dma.h"
#include "stm32f4xx_hal_uart.h"
#include "stm32f4xx_hal_spi.h"
#include "stm32f4xx_hal_i2c.h"

/* Public arguments are checked by the BSP before calling the SDK. */
#define assert_param(expr) ((void)0U)

#endif
