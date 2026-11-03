/* SPDX-License-Identifier: Apache-2.0 */
/* A second, host-only port exercises the common contract without STM32/HAL. */
#include <assert.h>
#include <stdio.h>
#include <string.h>
#include "emelino_spi.h"

typedef struct {
    uint64_t now;
    uint64_t last_deadline;
    unsigned configured, exchanges, asserts, releases, unclaims, recoveries;
    int32_t configure_error, transfer_error, assert_error, cleanup_error;
    int32_t cs_error, applied_hz;
    bool ready;
} Mock;
static Mock mocks[2];
static bool locked;
static unsigned locks, unlocks;
static int32_t validate_config(EmelinoSpiBus *bus, EmelaSpiConfig config) {
    (void)bus;
    return config.max_frequency_hz > 0 ? SPI_OK : SPI_INVALID_ARGUMENT;
}
static int32_t configure(EmelinoSpiBus *bus, EmelaSpiConfig config) {
    Mock *mock = bus->context;
    ++mock->configured;
    if (mock->configure_error) return mock->configure_error;
    mock->ready = true;
    mock->applied_hz = config.max_frequency_hz;
    return validate_config(bus, config);
}
static int32_t exchange(EmelinoSpiBus *bus, int32_t operation, const uint16_t *data,
        uint16_t length, uint16_t *output, uint64_t deadline) {
    Mock *mock = bus->context;
    ++mock->exchanges;
    mock->last_deadline = deadline;
    if (mock->now >= deadline) return SPI_TIMEOUT;
    if (mock->transfer_error) {
        bus->recovery_required = true;
        return mock->transfer_error;
    }
    for (uint16_t i = 0; i < length; ++i)
        if (output) output[i] = operation == EMELA_SPI_OPERATION_READ ? 0xff : data[i] ^ 0xa5;
    mock->now += length;
    return SPI_OK;
}
static int32_t recover(EmelinoSpiBus *bus, EmelaSpiConfig config) {
    Mock *mock = bus->context;
    ++mock->recoveries;
    return mock->ready ? configure(bus, config) : SPI_NOT_CONFIGURED;
}
static int32_t cs_configure(EmelinoSpiBus *bus, uint16_t slot, int32_t pin, int32_t level) {
    (void)slot; (void)pin;
    assert(level == 1);
    return ((Mock *)bus->context)->cs_error;
}
static int32_t cs_write(EmelinoSpiBus *bus, uint16_t slot, int32_t pin, int32_t level) {
    (void)slot; (void)pin;
    Mock *mock = bus->context;
    if (level) { ++mock->releases; return mock->cleanup_error; }
    ++mock->asserts;
    return mock->assert_error;
}
static void cs_release(EmelinoSpiBus *bus, uint16_t slot, int32_t pin) {
    (void)slot; (void)pin;
    ++((Mock *)bus->context)->unclaims;
}
static uint64_t now_us(EmelinoSpiBus *bus) { return ((Mock *)bus->context)->now; }
static bool delay_us(EmelinoSpiBus *bus, uint64_t delay, uint64_t deadline) {
    Mock *mock = bus->context;
    if (mock->now >= deadline || delay >= deadline - mock->now) return false;
    mock->now += delay;
    return true;
}
static const EmelinoSpiOps ops = {
    .validate_config = validate_config, .configure = configure, .exchange = exchange,
    .recover = recover, .cs_configure = cs_configure, .cs_write = cs_write,
    .cs_release = cs_release, .now_us = now_us, .delay_us = delay_us,
};
static EmelinoSpiBus buses[2] = {
    {.ops = &ops, .context = &mocks[0]}, {.ops = &ops, .context = &mocks[1]},
};
int32_t emelino_spi_lock(void) {
    if (locked) return SPI_BUSY;
    locked = true; ++locks; return SPI_OK;
}
void emelino_spi_unlock(void) { assert(locked); locked = false; ++unlocks; }
int32_t emelino_spi_bus(EmelaSpiId id, EmelinoSpiBus **bus) {
    assert(locked);
    if (id < EMELA_SPI_ID_SPI1 || id > EMELA_SPI_ID_SPI6) return SPI_INVALID_ARGUMENT;
    if (id > EMELA_SPI_ID_SPI2) return SPI_UNSUPPORTED;
    *bus = &buses[id]; return SPI_OK;
}
static EmelaSpiConfig config(int32_t hz) {
    return (EmelaSpiConfig){.mode = EMELA_SPI_MODE_MASTER,
        .direction = EMELA_SPI_DIRECTION_TWO_LINE, .data_size = EMELA_SPI_DATA_SIZE_BITS8,
        .max_frequency_hz = hz};
}
static EmelaSpiDeviceConfig device_config(int32_t id) {
    return (EmelaSpiDeviceConfig){.bus = id, .config = config(2000000),
        .cs = {.id = 40 + id}, .active = 0, .setup_us = 7, .hold_us = 11};
}
static void success(EmelaSpiTransactionStatus result) {
    assert(!result.primary_spi && !result.primary_gpio && !result.cleanup_gpio);
}
static EmelaSpiDevice create(int32_t id) {
    EmelaSpiDevice device;
    success(emela_spi_create_device(device_config(id), &device));
    return device;
}
static EmelaSpiTransactionStatus transfer(EmelaSpiDevice device, int64_t timeout) {
    uint16_t word = 42, length;
    EmelaSpiOperation operation = {.tag = EMELA_SPI_OPERATION_WRITE, .data = &word, .length = 1};
    return emela_spi_transaction(device, &operation, 1, timeout, 0, 0, &length);
}
static void basic(void) {
    assert(emela_spi_configure(0, config(1000000)) == SPI_OK);
    EmelaSpiDevice device = create(0);
    uint16_t words[] = {1, 2}, out[5] = {0}, lengths[3] = {99, 99, 99};
    EmelaSpiOperation operations[] = {{EMELA_SPI_OPERATION_WRITE, words, 2},
        {EMELA_SPI_OPERATION_READ, 0, 3}, {EMELA_SPI_OPERATION_TRANSFER, words, 2}};
    success(emela_spi_transaction(device, operations, 3, 1, out, 5, lengths));
    assert(lengths[0] == 0 && lengths[1] == 3 && lengths[2] == 2);
    assert(out[0] == 255 && out[2] == 255 && out[3] == 164 && out[4] == 167);
    assert(mocks[0].asserts == 1 && mocks[0].releases == 1 && mocks[0].exchanges == 3);
    assert(mocks[0].now == 25 && mocks[0].last_deadline == 989);
    assert(mocks[0].applied_hz == 2000000);
    assert(emela_spi_transmit(0, words, 2, 1) == SPI_OK);
    assert(mocks[0].applied_hz == 1000000); /* Device configuration does not replace raw setup. */
}
static void cleanup(void) {
    EmelaSpiDevice device = create(0);
    mocks[0].transfer_error = SPI_OVERRUN;
    mocks[0].cleanup_error = 6;
    EmelaSpiTransactionStatus result = transfer(device, 1);
    assert(result.primary_spi == SPI_OVERRUN && !result.primary_gpio && result.cleanup_gpio == 6);
    assert(mocks[0].releases == 1 && buses[0].recovery_required);
    assert(transfer(device, 1).primary_spi == SPI_INVALID_STATE);
    mocks[0].transfer_error = mocks[0].cleanup_error = 0;
    success(emela_spi_recover(0));
    assert(!buses[0].recovery_required);
    mocks[0].assert_error = 5;
    mocks[0].cleanup_error = 6;
    unsigned exchanges = mocks[0].exchanges, releases = mocks[0].releases;
    result = transfer(device, 1);
    assert(!result.primary_spi && result.primary_gpio == 5 && result.cleanup_gpio == 6);
    assert(mocks[0].exchanges == exchanges && mocks[0].releases == releases + 1);
}
static void deadline(void) {
    EmelaSpiDeviceConfig cfg = device_config(0);
    cfg.setup_us = 900; cfg.hold_us = 200;
    EmelaSpiDevice device;
    success(emela_spi_create_device(cfg, &device));
    assert(transfer(device, 1).primary_spi == SPI_TIMEOUT);
    assert(mocks[0].asserts == 0 && mocks[0].exchanges == 0); /* Reject before asserting CS. */
    cfg.setup_us = 800; cfg.hold_us = 200;
    success(emela_spi_create_device(cfg, &device));
    assert(transfer(device, 1).primary_spi == SPI_TIMEOUT);
    assert(mocks[0].asserts == 1 && mocks[0].releases == 1 && mocks[0].exchanges == 0);
    cfg.setup_us = 1; cfg.hold_us = 1;
    success(emela_spi_create_device(cfg, &device));
    mocks[0].now = UINT64_MAX - 100;
    success(transfer(device, 1)); /* Saturate the deadline instead of wrapping to zero. */
    assert(mocks[0].last_deadline == UINT64_MAX - 1);
}
static void registry(void) {
    EmelaSpiDevice device = {.slot = -1};
    mocks[0].configure_error = SPI_OTHER;
    assert(emela_spi_create_device(device_config(0), &device).primary_spi == SPI_OTHER);
    assert(device.slot == -1 && mocks[0].unclaims == 1);
    mocks[0].configure_error = 0;
    mocks[0].cs_error = 5;
    assert(emela_spi_create_device(device_config(0), &device).primary_gpio == 5);
    mocks[0].cs_error = 0;
    for (int i = 0; i < EMELINO_SPI_DEVICE_CAPACITY; ++i) assert(create(i % 2).slot == i);
    assert(emela_spi_create_device(device_config(0), &device).primary_spi == SPI_CAPACITY_EXCEEDED);
}
static void isolation(void) {
    assert(emela_spi_configure(0, config(1000000)) == SPI_OK);
    EmelaSpiDevice a = create(0), b = create(1);
    assert(emela_spi_transmit(1, 0, 0, 1) == SPI_NOT_CONFIGURED);
    mocks[0].transfer_error = SPI_MODE_FAULT;
    assert(transfer(a, 1).primary_spi == SPI_MODE_FAULT);
    success(transfer(b, 1));
    unsigned releases = mocks[1].releases;
    success(emela_spi_recover(0));
    assert(mocks[1].recoveries == 0 && mocks[1].releases == releases);
    assert(mocks[0].applied_hz == 1000000 && buses[0].current_config.max_frequency_hz == 1000000);
    assert(!buses[0].recovery_required && !buses[1].recovery_required);
}
static void invalid(void) {
    assert(emela_spi_configure(-1, config(1)) == SPI_INVALID_ARGUMENT);
    assert(emela_spi_configure(2, config(1)) == SPI_UNSUPPORTED);
    EmelaSpiDevice device = create(0);
    assert(transfer((EmelaSpiDevice){.slot = -1}, 1).primary_spi == SPI_INVALID_ARGUMENT);
    assert(transfer(device, 0).primary_spi == SPI_INVALID_ARGUMENT);
    uint16_t words[] = {1, 256}, lengths[2], out[2];
    EmelaSpiOperation ops2[] = {{EMELA_SPI_OPERATION_WRITE, words, 1},
        {EMELA_SPI_OPERATION_TRANSFER, words, 2}};
    assert(emela_spi_transaction(device, ops2, 2, 1, out, 2, lengths).primary_spi == SPI_INVALID_ARGUMENT);
    assert(mocks[0].asserts == 0 && mocks[0].exchanges == 0); /* Preflight the entire operation list. */
    ops2[1] = (EmelaSpiOperation){EMELA_SPI_OPERATION_READ, 0, 2};
    assert(emela_spi_transaction(device, ops2, 2, 1, out, 1, lengths).primary_spi == SPI_CAPACITY_EXCEEDED);
    assert(mocks[0].asserts == 0);
    locked = true;
    assert(transfer(device, 1).primary_spi == SPI_BUSY);
    assert(locked); locked = false;
}
int main(int argc, char **argv) {
    assert(argc == 2);
    if (!strcmp(argv[1], "basic")) basic();
    else if (!strcmp(argv[1], "cleanup")) cleanup();
    else if (!strcmp(argv[1], "deadline")) deadline();
    else if (!strcmp(argv[1], "registry")) registry();
    else if (!strcmp(argv[1], "isolation")) isolation();
    else if (!strcmp(argv[1], "invalid")) invalid();
    else return 1;
    assert(!locked && locks == unlocks);
    printf("PASS: common SPI %s\n", argv[1]);
    return 0;
}
