/* SPDX-License-Identifier: Apache-2.0 */
#include "emelino_spi.h"

typedef struct {
    bool used;
    EmelaSpiDeviceConfig config;
} SpiDeviceSlot;

static SpiDeviceSlot devices[EMELINO_SPI_DEVICE_CAPACITY];

static int32_t enter(EmelaSpiId id, EmelinoSpiBus **bus) {
    int32_t status = emelino_spi_lock();
    if (status) return status;
    status = emelino_spi_bus(id, bus);
    if (status) emelino_spi_unlock();
    return status;
}

static EmelaSpiTransactionStatus spi_failure(int32_t status) {
    return (EmelaSpiTransactionStatus){.primary_spi = status};
}

static EmelaSpiTransactionStatus gpio_failure(int32_t status) {
    return (EmelaSpiTransactionStatus){.primary_gpio = status};
}

static int32_t apply_config(EmelinoSpiBus *bus, EmelaSpiConfig config) {
    int32_t status = bus->ops->configure(bus, config);
    if (!status) bus->current_config = config;
    return status;
}

static uint64_t deadline_after(EmelinoSpiBus *bus, int64_t timeout_ms) {
    uint64_t now = bus->ops->now_us(bus);
    return (uint64_t)timeout_ms > (UINT64_MAX - now) / 1000
        ? UINT64_MAX : now + (uint64_t)timeout_ms * 1000;
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

int32_t emela_spi_configure(EmelaSpiId id, EmelaSpiConfig config) {
    EmelinoSpiBus *bus;
    int32_t status = enter(id, &bus);
    if (status) return status;
    status = bus->recovery_required ? SPI_INVALID_STATE : apply_config(bus, config);
    if (!status) {
        bus->raw_config = config;
        bus->raw_configured = true;
    }
    emelino_spi_unlock();
    return status;
}

static int32_t raw_transfer(EmelaSpiId id, int32_t operation, const uint16_t *data, uint16_t length,
                            int64_t timeout_ms, uint16_t *output) {
    EmelinoSpiBus *bus;
    int32_t status = enter(id, &bus);
    if (status) return status;
    if (timeout_ms <= 0 || (uint64_t)timeout_ms > UINT32_MAX) status = SPI_INVALID_ARGUMENT;
    else if (!bus->raw_configured) status = SPI_NOT_CONFIGURED;
    else if (bus->recovery_required) status = SPI_INVALID_STATE;
    else status = validate_transfer(operation, data, length, bus->raw_config);
    if (!status) {
        uint64_t deadline = deadline_after(bus, timeout_ms);
        status = apply_config(bus, bus->raw_config);
        if (!status) status = bus->ops->exchange(bus, operation, data, length, output, deadline);
    }
    emelino_spi_unlock();
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
    EmelinoSpiBus *bus;
    int32_t status = enter(config.bus, &bus);
    if (status) return spi_failure(status);
    status = bus->ops->validate_config(bus, config.config);
    if (output == 0 || config.setup_us < 0 || config.hold_us < 0
        || config.active < 0 || config.active > 1) status = SPI_INVALID_ARGUMENT;
    if (bus->recovery_required) status = SPI_INVALID_STATE;
    int slot = -1;
    for (int i = 0; i < EMELINO_SPI_DEVICE_CAPACITY; ++i) if (!devices[i].used) { slot = i; break; }
    if (!status && slot < 0) status = SPI_CAPACITY_EXCEEDED;
    EmelaSpiTransactionStatus result = spi_failure(status);
    if (!status) {
        int32_t gpio_status = bus->ops->cs_configure(bus, (uint16_t)slot, config.cs.id, !config.active);
        if (gpio_status) result = gpio_failure(gpio_status);
        else {
            status = apply_config(bus, config.config);
            if (status) {
                bus->ops->cs_release(bus, (uint16_t)slot, config.cs.id);
                result = spi_failure(status);
            } else {
                devices[slot] = (SpiDeviceSlot){.used = true, .config = config};
                output->slot = slot;
            }
        }
    }
    emelino_spi_unlock();
    return result;
}

EmelaSpiTransactionStatus emela_spi_transaction(EmelaSpiDevice device, const EmelaSpiOperation *operations,
    uint16_t count, int64_t timeout_ms, uint16_t *output, uint16_t capacity, uint16_t *lengths) {
    int32_t status = emelino_spi_lock();
    if (status) return spi_failure(status);
    if (device.slot < 0 || device.slot >= EMELINO_SPI_DEVICE_CAPACITY || !devices[device.slot].used) {
        emelino_spi_unlock();
        return spi_failure(SPI_INVALID_ARGUMENT);
    }
    EmelaSpiDeviceConfig config = devices[device.slot].config;
    EmelinoSpiBus *bus;
    status = emelino_spi_bus(config.bus, &bus);
    if (status) { emelino_spi_unlock(); return spi_failure(status); }
    if (timeout_ms <= 0 || (uint64_t)timeout_ms > UINT32_MAX
        || (count != 0 && (operations == 0 || lengths == 0))) status = SPI_INVALID_ARGUMENT;
    else if (bus->recovery_required) status = SPI_INVALID_STATE;
    uint32_t total = 0;
    for (uint16_t i = 0; !status && i < count; ++i) {
        status = validate_transfer(operations[i].tag, operations[i].data, operations[i].length, config.config);
        if (operations[i].tag != EMELA_SPI_OPERATION_WRITE) total += operations[i].length;
    }
    if (!status && total > capacity) status = SPI_CAPACITY_EXCEEDED;
    if (!status && total != 0 && output == 0) status = SPI_INVALID_ARGUMENT;
    if (!status && (uint64_t)config.setup_us + (uint64_t)config.hold_us > (uint64_t)timeout_ms * 1000)
        status = SPI_TIMEOUT;
    if (status) { emelino_spi_unlock(); return spi_failure(status); }

    uint64_t deadline = deadline_after(bus, timeout_ms);
    uint64_t transfer_deadline = deadline - (uint64_t)config.hold_us;
    EmelaSpiTransactionStatus result = spi_failure(apply_config(bus, config.config));
    if (!result.primary_spi) {
        result.primary_gpio = bus->ops->cs_write(bus, (uint16_t)device.slot, config.cs.id, config.active);
        if (!result.primary_gpio) {
            if (!bus->ops->delay_us(bus, (uint64_t)config.setup_us, transfer_deadline)) result.primary_spi = SPI_TIMEOUT;
            uint16_t offset = 0;
            for (uint16_t i = 0; !result.primary_spi && i < count; ++i) {
                bool writes = operations[i].tag == EMELA_SPI_OPERATION_WRITE;
                lengths[i] = writes ? 0 : operations[i].length;
                result.primary_spi = bus->ops->exchange(bus, operations[i].tag, operations[i].data,
                    operations[i].length, writes || operations[i].length == 0 ? 0 : output + offset, transfer_deadline);
                offset += lengths[i];
            }
            /* Reserve hold time up front, but never stall cleanup after the caller's deadline. */
            if (!bus->ops->delay_us(bus, (uint64_t)config.hold_us, deadline) && !result.primary_spi)
                result.primary_spi = SPI_TIMEOUT;
        }
        /* A failed assertion can still have changed the output: always attempt release. */
        result.cleanup_gpio = bus->ops->cs_write(bus, (uint16_t)device.slot, config.cs.id, !config.active);
    }
    if (result.cleanup_gpio) bus->recovery_required = true;
    emelino_spi_unlock();
    return result;
}

EmelaSpiTransactionStatus emela_spi_recover(EmelaSpiId id) {
    EmelinoSpiBus *bus;
    int32_t status = enter(id, &bus);
    if (status) return spi_failure(status);
    EmelaSpiTransactionStatus result = {0};
    EmelaSpiConfig config = bus->raw_configured ? bus->raw_config : bus->current_config;
    result.primary_spi = bus->ops->recover(bus, config);
    if (!result.primary_spi) bus->current_config = config;
    if (result.primary_spi != SPI_NOT_CONFIGURED) {
        for (int i = 0; i < EMELINO_SPI_DEVICE_CAPACITY; ++i) if (devices[i].used && devices[i].config.bus == id) {
            EmelaSpiDeviceConfig config = devices[i].config;
            int32_t failure = bus->ops->cs_write(bus, (uint16_t)i, config.cs.id, !config.active);
            if (failure) {
                if (!result.primary_spi && !result.primary_gpio) result.primary_gpio = failure;
                else if (!result.cleanup_gpio) result.cleanup_gpio = failure;
            }
        }
        bus->recovery_required = result.primary_spi || result.primary_gpio || result.cleanup_gpio;
    }
    emelino_spi_unlock();
    return result;
}
