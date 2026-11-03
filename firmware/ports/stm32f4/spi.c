/* SPDX-License-Identifier: Apache-2.0 */
#include "board_config.h"
#include "bsp.h"
#include "emelino_spi.h"
#include "gpio_support.h"

typedef struct {
    SPI_HandleTypeDef handle;
    bool pins_claimed;
} Stm32Spi;

static Stm32Spi controller;
static bool occupied;
static const int32_t bus_pins[] = {EMELA_SPI1_SCK_PIN, EMELA_SPI1_MISO_PIN, EMELA_SPI1_MOSI_PIN};

static int32_t valid_config(EmelaSpiConfig config, uint32_t *prescaler) {
    if (config.mode < EMELA_SPI_MODE_MASTER || config.mode > EMELA_SPI_MODE_SLAVE
        || config.direction < EMELA_SPI_DIRECTION_TWO_LINE || config.direction > EMELA_SPI_DIRECTION_ONE_LINE
        || config.data_size < EMELA_SPI_DATA_SIZE_BITS8 || config.data_size > EMELA_SPI_DATA_SIZE_BITS16
        || config.polarity < EMELA_SPI_CLOCK_POLARITY_IDLE_LOW || config.polarity > EMELA_SPI_CLOCK_POLARITY_IDLE_HIGH
        || config.phase < EMELA_SPI_CLOCK_PHASE_FIRST_EDGE || config.phase > EMELA_SPI_CLOCK_PHASE_SECOND_EDGE
        || config.bit_order < EMELA_SPI_BIT_ORDER_MOST_SIGNIFICANT_FIRST || config.bit_order > EMELA_SPI_BIT_ORDER_LEAST_SIGNIFICANT_FIRST)
        return SPI_INVALID_ARGUMENT;
    /* This board mapping has no slave NSS input. */
    if (config.mode != EMELA_SPI_MODE_MASTER) return SPI_UNSUPPORTED;
    if (config.max_frequency_hz <= 0) return SPI_INVALID_ARGUMENT;
    uint32_t clock = HAL_RCC_GetPCLK2Freq();
    for (uint32_t shift = 0; shift < 8; ++shift) {
        uint32_t divisor = UINT32_C(2) << shift;
        if ((uint64_t)clock <= (uint64_t)(uint32_t)config.max_frequency_hz * divisor) {
            *prescaler = shift << SPI_CR1_BR_Pos;
            return SPI_OK;
        }
    }
    return SPI_UNSUPPORTED;
}

static int32_t apply_config(EmelinoSpiBus *bus, EmelaSpiConfig config) {
    Stm32Spi *state = bus->context;
    SPI_HandleTypeDef *spi = &state->handle;
    uint32_t prescaler;
    int32_t status = valid_config(config, &prescaler);
    if (status != SPI_OK) return status;
    if (!state->pins_claimed) {
        int32_t pin_status = stm32f4_pins_claim(STM32F4_OWNER_SPI1, bus_pins, 3);
        if (pin_status) return pin_status == 5 ? SPI_RESOURCE_CONFLICT : SPI_INVALID_ARGUMENT;
        GPIO_InitTypeDef init = {
            .Mode = GPIO_MODE_AF_PP, .Pull = GPIO_NOPULL,
            .Speed = GPIO_SPEED_FREQ_VERY_HIGH, .Alternate = EMELA_SPI1_AF,
        };
        for (uint32_t i = 0; i < 3; ++i) {
            uint32_t port = (uint32_t)bus_pins[i] / 16;
            stm32f4_enable_gpio_clock(port);
            init.Pin = UINT32_C(1) << ((uint32_t)bus_pins[i] % 16);
            HAL_GPIO_Init(stm32f4_gpio_port(port), &init);
        }
        __HAL_RCC_SPI1_CLK_ENABLE();
        state->pins_claimed = true;
    }
    spi->Instance = SPI1;
    spi->Init = (SPI_InitTypeDef){
        .Mode = SPI_MODE_MASTER,
        .Direction = config.direction == EMELA_SPI_DIRECTION_TWO_LINE ? SPI_DIRECTION_2LINES
            : config.direction == EMELA_SPI_DIRECTION_TWO_LINE_RECEIVE_ONLY ? SPI_DIRECTION_2LINES_RXONLY
            : SPI_DIRECTION_1LINE,
        .DataSize = config.data_size == EMELA_SPI_DATA_SIZE_BITS8 ? SPI_DATASIZE_8BIT : SPI_DATASIZE_16BIT,
        .CLKPolarity = config.polarity == EMELA_SPI_CLOCK_POLARITY_IDLE_LOW ? SPI_POLARITY_LOW : SPI_POLARITY_HIGH,
        .CLKPhase = config.phase == EMELA_SPI_CLOCK_PHASE_FIRST_EDGE ? SPI_PHASE_1EDGE : SPI_PHASE_2EDGE,
        .NSS = SPI_NSS_SOFT, .BaudRatePrescaler = prescaler,
        .FirstBit = config.bit_order == EMELA_SPI_BIT_ORDER_MOST_SIGNIFICANT_FIRST ? SPI_FIRSTBIT_MSB : SPI_FIRSTBIT_LSB,
        .TIMode = SPI_TIMODE_DISABLE, .CRCCalculation = SPI_CRCCALCULATION_DISABLE,
        .CRCPolynomial = 7,
    };
    if (HAL_SPI_Init(spi) != HAL_OK) return SPI_OTHER;
    return SPI_OK;
}

static int32_t validate_config(EmelinoSpiBus *bus, EmelaSpiConfig config) {
    (void)bus;
    uint32_t prescaler;
    return valid_config(config, &prescaler);
}

static uint32_t remaining_ms(uint64_t deadline) {
    uint64_t now = stm32f4_now_us();
    if (now >= deadline) return 0;
    uint64_t remaining = deadline - now;
    uint64_t millis = remaining / 1000 + (remaining % 1000 != 0);
    return millis >= HAL_MAX_DELAY ? HAL_MAX_DELAY - 1 : (uint32_t)millis;
}

static int32_t hal_status(SPI_HandleTypeDef *spi, HAL_StatusTypeDef status) {
    if ((spi->ErrorCode & HAL_SPI_ERROR_MODF) || (spi->Instance->SR & SPI_SR_MODF)) return SPI_MODE_FAULT;
    if ((spi->ErrorCode & HAL_SPI_ERROR_OVR) || (spi->Instance->SR & SPI_SR_OVR)) return SPI_OVERRUN;
    if (status == HAL_TIMEOUT) return SPI_TIMEOUT;
    if (status == HAL_BUSY) return SPI_BUSY;
    return status == HAL_OK ? SPI_OK : SPI_OTHER;
}

static int32_t exchange(EmelinoSpiBus *bus, int32_t operation, const uint16_t *data, uint16_t length,
                        uint16_t *output, uint64_t deadline) {
    Stm32Spi *state = bus->context;
    SPI_HandleTypeDef *spi = &state->handle;
    EmelaSpiConfig current_config = bus->current_config;
    for (uint16_t i = 0; i < length; ++i) {
        uint32_t timeout = remaining_ms(deadline);
        if (!timeout) return SPI_TIMEOUT;
        uint16_t tx = operation == EMELA_SPI_OPERATION_READ
            ? (current_config.data_size == EMELA_SPI_DATA_SIZE_BITS8 ? UINT8_MAX : UINT16_MAX) : data[i];
        uint16_t rx = 0;
        HAL_StatusTypeDef result;
        /* One word at a time keeps uint16_t public buffers independent of HAL's byte stride. */
        if (current_config.direction == EMELA_SPI_DIRECTION_TWO_LINE)
            result = HAL_SPI_TransmitReceive(spi, (const uint8_t *)&tx, (uint8_t *)&rx, 1, timeout);
        else if (operation == EMELA_SPI_OPERATION_WRITE)
            result = HAL_SPI_Transmit(spi, (const uint8_t *)&tx, 1, timeout);
        else
            result = HAL_SPI_Receive(spi, (uint8_t *)&rx, 1, timeout);
        int32_t status = hal_status(spi, result);
        if (status == SPI_OK && stm32f4_now_us() >= deadline) status = SPI_TIMEOUT;
        if (status) {
            bus->recovery_required = true;
            return status;
        }
        if (output != 0) output[i] = rx;
    }
    return SPI_OK;
}

static int32_t recover(EmelinoSpiBus *bus, EmelaSpiConfig config) {
    Stm32Spi *state = bus->context;
    SPI_HandleTypeDef *spi = &state->handle;
    if (!state->pins_claimed) return SPI_NOT_CONFIGURED;
    __HAL_SPI_DISABLE(spi);
    __HAL_SPI_CLEAR_OVRFLAG(spi);
    __HAL_SPI_CLEAR_MODFFLAG(spi);
    __HAL_RCC_SPI1_FORCE_RESET();
    __HAL_RCC_SPI1_RELEASE_RESET();
    spi->State = HAL_SPI_STATE_RESET;
    spi->Lock = HAL_UNLOCKED;
    return apply_config(bus, config);
}

static int32_t cs_configure(EmelinoSpiBus *bus, uint16_t slot, int32_t pin, int32_t level) {
    (void)bus;
    return stm32f4_gpio_output_owned((uint16_t)(STM32F4_OWNER_SPI_CS_BASE + slot), pin, level);
}
static int32_t cs_write(EmelinoSpiBus *bus, uint16_t slot, int32_t pin, int32_t level) {
    (void)bus;
    return stm32f4_gpio_write_owned((uint16_t)(STM32F4_OWNER_SPI_CS_BASE + slot), pin, level);
}
static void cs_release(EmelinoSpiBus *bus, uint16_t slot, int32_t pin) {
    (void)bus;
    stm32f4_gpio_release_owned((uint16_t)(STM32F4_OWNER_SPI_CS_BASE + slot), pin);
}
static uint64_t now_us(EmelinoSpiBus *bus) {
    (void)bus;
    return stm32f4_now_us();
}
static bool delay_us(EmelinoSpiBus *bus, uint64_t delay, uint64_t deadline) {
    (void)bus;
    return stm32f4_delay_us(delay, deadline);
}
static const EmelinoSpiOps operations = {
    .validate_config = validate_config, .configure = apply_config, .exchange = exchange,
    .recover = recover, .cs_configure = cs_configure, .cs_write = cs_write,
    .cs_release = cs_release, .now_us = now_us, .delay_us = delay_us,
};
static EmelinoSpiBus bus1 = {.ops = &operations, .context = &controller};

int32_t emelino_spi_lock(void) {
    if (stm32f4_exception_number() != 0 || __get_PRIMASK() != 0) return SPI_INVALID_STATE;
    uint32_t previous = stm32f4_interrupt_lock();
    bool was_occupied = occupied;
    if (!was_occupied) occupied = true;
    stm32f4_interrupt_unlock(previous);
    return was_occupied ? SPI_BUSY : SPI_OK;
}
void emelino_spi_unlock(void) {
    uint32_t previous = stm32f4_interrupt_lock();
    occupied = false;
    stm32f4_interrupt_unlock(previous);
}
int32_t emelino_spi_bus(EmelaSpiId id, EmelinoSpiBus **bus) {
    if (id < EMELA_SPI_ID_SPI1 || id > EMELA_SPI_ID_SPI6) return SPI_INVALID_ARGUMENT;
    if (id != EMELA_SPI_ID_SPI1) return SPI_UNSUPPORTED;
    *bus = &bus1;
    return SPI_OK;
}
