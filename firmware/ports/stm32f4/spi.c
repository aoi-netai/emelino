/* SPDX-License-Identifier: Apache-2.0 */
#include <stdbool.h>
#include <stdint.h>

#include "board_config.h"
#include "bsp.h"
#include "emela_spi.h"
#include "gpio_support.h"
#include "registers.h"

/* 0 succeeds; nonzero values are SpiErrorKind ordinal + 1. */
enum {
    SPI_OK, SPI_INVALID_ARGUMENT, SPI_CAPACITY_EXCEEDED, SPI_NOT_CONFIGURED,
    SPI_UNSUPPORTED, SPI_TIMEOUT, SPI_BUSY, SPI_RESOURCE_CONFLICT,
    SPI_INVALID_STATE, SPI_MODE_FAULT, SPI_OVERRUN, SPI_OTHER,
};
enum { DEVICE_CAPACITY = 8 };

typedef struct {
    bool used;
    EmelaSpiDeviceConfig config;
} SpiDeviceSlot;

static SPI_HandleTypeDef spi;
static bool pins_claimed;
static bool raw_configured;
static bool busy;
static bool recovery_required;
static EmelaSpiConfig raw_config;
static EmelaSpiConfig current_config;
static SpiDeviceSlot devices[DEVICE_CAPACITY];
static const int32_t bus_pins[] = {EMELA_SPI1_SCK_PIN, EMELA_SPI1_MISO_PIN, EMELA_SPI1_MOSI_PIN};

static EmelaSpiTransactionStatus spi_failure(int32_t status) {
    return (EmelaSpiTransactionStatus){.primary_spi = status};
}

static EmelaSpiTransactionStatus gpio_failure(int32_t status) {
    return (EmelaSpiTransactionStatus){.primary_gpio = status};
}

static int32_t valid_id(EmelaSpiId id) {
    return id < EMELA_SPI_ID_SPI1 || id > EMELA_SPI_ID_SPI6 ? SPI_INVALID_ARGUMENT
        : id != EMELA_SPI_ID_SPI1 ? SPI_UNSUPPORTED : SPI_OK;
}

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

static int32_t apply_config(EmelaSpiConfig config) {
    uint32_t prescaler;
    int32_t status = valid_config(config, &prescaler);
    if (status != SPI_OK) return status;
    if (!pins_claimed) {
        int32_t pin_status = stm32f4_pins_claim(STM32F4_OWNER_SPI1, bus_pins, 3);
        if (pin_status) return pin_status == 5 ? SPI_RESOURCE_CONFLICT : SPI_INVALID_ARGUMENT;
        __HAL_RCC_GPIOA_CLK_ENABLE();
        GPIO_InitTypeDef init = {
            .Pin = (UINT32_C(1) << EMELA_SPI1_SCK_PIN) | (UINT32_C(1) << EMELA_SPI1_MISO_PIN) | (UINT32_C(1) << EMELA_SPI1_MOSI_PIN),
            .Mode = GPIO_MODE_AF_PP, .Pull = GPIO_NOPULL,
            .Speed = GPIO_SPEED_FREQ_VERY_HIGH, .Alternate = EMELA_SPI1_AF,
        };
        HAL_GPIO_Init(GPIOA, &init);
        __HAL_RCC_SPI1_CLK_ENABLE();
        pins_claimed = true;
    }
    spi.Instance = SPI1;
    spi.Init = (SPI_InitTypeDef){
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
    if (HAL_SPI_Init(&spi) != HAL_OK) return SPI_OTHER;
    current_config = config;
    return SPI_OK;
}

static int32_t enter(EmelaSpiId id) {
    int32_t status = valid_id(id);
    if (status) return status;
    if (stm32f4_exception_number() != 0 || __get_PRIMASK() != 0) return SPI_INVALID_STATE;
    if (busy) return SPI_BUSY;
    busy = true;
    return SPI_OK;
}

static uint64_t deadline_after(int64_t timeout_ms) {
    uint64_t now = stm32f4_now_us();
    return (uint64_t)timeout_ms > (UINT64_MAX - now) / 1000
        ? UINT64_MAX : now + (uint64_t)timeout_ms * 1000;
}

static uint32_t remaining_ms(uint64_t deadline) {
    uint64_t now = stm32f4_now_us();
    if (now >= deadline) return 0;
    uint64_t remaining = deadline - now;
    uint64_t millis = remaining / 1000 + (remaining % 1000 != 0);
    return millis >= HAL_MAX_DELAY ? HAL_MAX_DELAY - 1 : (uint32_t)millis;
}

static int32_t hal_status(HAL_StatusTypeDef status) {
    if ((spi.ErrorCode & HAL_SPI_ERROR_MODF) || (spi.Instance->SR & SPI_SR_MODF)) return SPI_MODE_FAULT;
    if ((spi.ErrorCode & HAL_SPI_ERROR_OVR) || (spi.Instance->SR & SPI_SR_OVR)) return SPI_OVERRUN;
    if (status == HAL_TIMEOUT) return SPI_TIMEOUT;
    if (status == HAL_BUSY) return SPI_BUSY;
    return status == HAL_OK ? SPI_OK : SPI_OTHER;
}

static int32_t validate_transfer(int32_t operation, const uint16_t *data, uint16_t length,
                                 EmelaSpiConfig config) {
    if (operation < EMELA_SPI_OPERATION_WRITE || operation > EMELA_SPI_OPERATION_TRANSFER)
        return SPI_INVALID_ARGUMENT;
    if (operation != EMELA_SPI_OPERATION_READ && length != 0 && data == 0) return SPI_INVALID_ARGUMENT;
    if (config.direction == EMELA_SPI_DIRECTION_TWO_LINE_RECEIVE_ONLY && operation != EMELA_SPI_OPERATION_READ)
        return SPI_INVALID_STATE;
    if (config.direction == EMELA_SPI_DIRECTION_ONE_LINE && operation == EMELA_SPI_OPERATION_TRANSFER)
        return SPI_INVALID_STATE;
    if (config.data_size == EMELA_SPI_DATA_SIZE_BITS8 && operation != EMELA_SPI_OPERATION_READ) {
        for (uint16_t i = 0; i < length; ++i) if (data[i] > UINT8_MAX) return SPI_INVALID_ARGUMENT;
    }
    return SPI_OK;
}

static int32_t exchange(int32_t operation, const uint16_t *data, uint16_t length,
                        uint16_t *output, uint64_t deadline) {
    for (uint16_t i = 0; i < length; ++i) {
        uint32_t timeout = remaining_ms(deadline);
        if (!timeout) return SPI_TIMEOUT;
        uint16_t tx = operation == EMELA_SPI_OPERATION_READ
            ? (current_config.data_size == EMELA_SPI_DATA_SIZE_BITS8 ? UINT8_MAX : UINT16_MAX) : data[i];
        uint16_t rx = 0;
        HAL_StatusTypeDef result;
        /* One word at a time keeps uint16_t public buffers independent of HAL's byte stride. */
        if (current_config.direction == EMELA_SPI_DIRECTION_TWO_LINE)
            result = HAL_SPI_TransmitReceive(&spi, (const uint8_t *)&tx, (uint8_t *)&rx, 1, timeout);
        else if (operation == EMELA_SPI_OPERATION_WRITE)
            result = HAL_SPI_Transmit(&spi, (const uint8_t *)&tx, 1, timeout);
        else
            result = HAL_SPI_Receive(&spi, (uint8_t *)&rx, 1, timeout);
        int32_t status = hal_status(result);
        if (status == SPI_OK && stm32f4_now_us() >= deadline) status = SPI_TIMEOUT;
        if (status) {
            recovery_required = true;
            return status;
        }
        if (output != 0) output[i] = rx;
    }
    return SPI_OK;
}

int32_t emela_spi_configure(EmelaSpiId id, EmelaSpiConfig config) {
    int32_t status = enter(id);
    if (status) return status;
    status = recovery_required ? SPI_INVALID_STATE : apply_config(config);
    if (!status) {
        raw_config = config;
        raw_configured = true;
    }
    busy = false;
    return status;
}

static int32_t raw_transfer(EmelaSpiId id, int32_t operation, const uint16_t *data, uint16_t length,
                            int64_t timeout_ms, uint16_t *output) {
    int32_t status = enter(id);
    if (status) return status;
    if (timeout_ms <= 0 || (uint64_t)timeout_ms > UINT32_MAX) status = SPI_INVALID_ARGUMENT;
    else if (!raw_configured) status = SPI_NOT_CONFIGURED;
    else if (recovery_required) status = SPI_INVALID_STATE;
    else status = validate_transfer(operation, data, length, raw_config);
    if (!status) {
        uint64_t deadline = deadline_after(timeout_ms);
        status = apply_config(raw_config);
        if (!status) status = exchange(operation, data, length, output, deadline);
    }
    busy = false;
    return status;
}

int32_t emela_spi_transmit(EmelaSpiId id, const uint16_t *data, uint16_t length, int64_t timeout_ms) {
    return raw_transfer(id, EMELA_SPI_OPERATION_WRITE, data, length, timeout_ms, 0);
}

int32_t emela_spi_receive(EmelaSpiId id, int32_t size, int64_t timeout_ms,
                         uint16_t *output, uint16_t capacity, uint16_t *length) {
    if (size < 0 || length == 0 || (size != 0 && output == 0)) return SPI_INVALID_ARGUMENT;
    if ((uint32_t)size > capacity) return SPI_CAPACITY_EXCEEDED;
    *length = 0;
    int32_t status = raw_transfer(id, EMELA_SPI_OPERATION_READ, 0, (uint16_t)size, timeout_ms, output);
    if (!status) *length = (uint16_t)size;
    return status;
}

int32_t emela_spi_transfer(EmelaSpiId id, const uint16_t *data, uint16_t size, int64_t timeout_ms,
                          uint16_t *output, uint16_t capacity, uint16_t *length) {
    if (length == 0 || (size != 0 && output == 0)) return SPI_INVALID_ARGUMENT;
    if (size > capacity) return SPI_CAPACITY_EXCEEDED;
    *length = 0;
    int32_t status = raw_transfer(id, EMELA_SPI_OPERATION_TRANSFER, data, size, timeout_ms, output);
    if (!status) *length = size;
    return status;
}

EmelaSpiTransactionStatus emela_spi_create_device(EmelaSpiDeviceConfig config, EmelaSpiDevice *output) {
    int32_t status = enter(config.bus);
    if (status) return spi_failure(status);
    uint32_t prescaler;
    status = valid_config(config.config, &prescaler);
    if (output == 0 || config.setup_us < 0 || config.hold_us < 0
        || config.active < 0 || config.active > 1) status = SPI_INVALID_ARGUMENT;
    if (recovery_required) status = SPI_INVALID_STATE;
    int slot = -1;
    for (int i = 0; i < DEVICE_CAPACITY; ++i) if (!devices[i].used) { slot = i; break; }
    if (!status && slot < 0) status = SPI_CAPACITY_EXCEEDED;
    EmelaSpiTransactionStatus result = spi_failure(status);
    if (!status) {
        uint16_t owner = (uint16_t)(STM32F4_OWNER_SPI_CS_BASE + slot);
        int32_t gpio_status = stm32f4_gpio_output_owned(owner, config.cs.id, !config.active);
        if (gpio_status) result = gpio_failure(gpio_status);
        else {
            status = apply_config(config.config);
            if (status) {
                stm32f4_gpio_release_owned(owner, config.cs.id);
                result = spi_failure(status);
            } else {
                devices[slot] = (SpiDeviceSlot){.used = true, .config = config};
                output->slot = slot;
            }
        }
    }
    busy = false;
    return result;
}

EmelaSpiTransactionStatus emela_spi_transaction(EmelaSpiDevice device, const EmelaSpiOperation *operations,
    uint16_t count, int64_t timeout_ms, uint16_t *output, uint16_t capacity, uint16_t *lengths) {
    if (device.slot < 0 || device.slot >= DEVICE_CAPACITY || !devices[device.slot].used)
        return spi_failure(SPI_INVALID_ARGUMENT);
    EmelaSpiDeviceConfig config = devices[device.slot].config;
    int32_t status = enter(config.bus);
    if (status) return spi_failure(status);
    if (timeout_ms <= 0 || (uint64_t)timeout_ms > UINT32_MAX
        || (count != 0 && (operations == 0 || lengths == 0))) status = SPI_INVALID_ARGUMENT;
    else if (recovery_required) status = SPI_INVALID_STATE;
    uint32_t total = 0;
    for (uint16_t i = 0; !status && i < count; ++i) {
        status = validate_transfer(operations[i].tag, operations[i].data, operations[i].length, config.config);
        if (operations[i].tag != EMELA_SPI_OPERATION_WRITE) total += operations[i].length;
    }
    if (!status && total > capacity) status = SPI_CAPACITY_EXCEEDED;
    if (!status && total != 0 && output == 0) status = SPI_INVALID_ARGUMENT;
    if (!status && (uint64_t)config.setup_us + (uint64_t)config.hold_us > (uint64_t)timeout_ms * 1000)
        status = SPI_TIMEOUT;
    if (status) { busy = false; return spi_failure(status); }

    uint64_t deadline = deadline_after(timeout_ms);
    uint64_t transfer_deadline = deadline - (uint64_t)config.hold_us;
    EmelaSpiTransactionStatus result = spi_failure(apply_config(config.config));
    uint16_t owner = (uint16_t)(STM32F4_OWNER_SPI_CS_BASE + device.slot);
    if (!result.primary_spi) {
        result.primary_gpio = stm32f4_gpio_write_owned(owner, config.cs.id, config.active);
        if (!result.primary_gpio) {
            if (!stm32f4_delay_us((uint64_t)config.setup_us, transfer_deadline)) result.primary_spi = SPI_TIMEOUT;
            uint16_t offset = 0;
            for (uint16_t i = 0; !result.primary_spi && i < count; ++i) {
                bool writes = operations[i].tag == EMELA_SPI_OPERATION_WRITE;
                lengths[i] = writes ? 0 : operations[i].length;
                result.primary_spi = exchange(operations[i].tag, operations[i].data,
                    operations[i].length, writes || operations[i].length == 0 ? 0 : output + offset, transfer_deadline);
                offset += lengths[i];
            }
            /* Reserve hold time up front, but never stall cleanup after the caller's deadline. */
            if (!stm32f4_delay_us((uint64_t)config.hold_us, deadline) && !result.primary_spi)
                result.primary_spi = SPI_TIMEOUT;
        }
        /* A failed assertion can still have changed the output: always attempt release. */
        result.cleanup_gpio = stm32f4_gpio_write_owned(owner, config.cs.id, !config.active);
    }
    if (result.cleanup_gpio) recovery_required = true;
    busy = false;
    return result;
}

EmelaSpiTransactionStatus emela_spi_recover(EmelaSpiId id) {
    int32_t status = enter(id);
    if (status) return spi_failure(status);
    EmelaSpiTransactionStatus result = {0};
    if (!pins_claimed) result.primary_spi = SPI_NOT_CONFIGURED;
    else {
        __HAL_SPI_DISABLE(&spi);
        __HAL_SPI_CLEAR_OVRFLAG(&spi);
        __HAL_SPI_CLEAR_MODFFLAG(&spi);
        __HAL_RCC_SPI1_FORCE_RESET();
        __HAL_RCC_SPI1_RELEASE_RESET();
        spi.State = HAL_SPI_STATE_RESET;
        spi.Lock = HAL_UNLOCKED;
        result.primary_spi = apply_config(raw_configured ? raw_config : current_config);
        for (int i = 0; i < DEVICE_CAPACITY; ++i) if (devices[i].used) {
            EmelaSpiDeviceConfig config = devices[i].config;
            int32_t failure = stm32f4_gpio_write_owned((uint16_t)(STM32F4_OWNER_SPI_CS_BASE + i), config.cs.id, !config.active);
            if (failure) {
                if (!result.primary_spi && !result.primary_gpio) result.primary_gpio = failure;
                else if (!result.cleanup_gpio) result.cleanup_gpio = failure;
            }
        }
        recovery_required = result.primary_spi || result.primary_gpio || result.cleanup_gpio;
    }
    busy = false;
    return result;
}
