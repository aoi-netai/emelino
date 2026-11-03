/* SPDX-License-Identifier: Apache-2.0 */
#ifndef EMELINO_COMMON_SPI_H
#define EMELINO_COMMON_SPI_H
#include <stdbool.h>
#include <stdint.h>
#include "emela_spi.h"

/* Public C status values: SpiErrorKind ordinal + 1, zero succeeds. */
enum {
    SPI_OK, SPI_INVALID_ARGUMENT, SPI_CAPACITY_EXCEEDED, SPI_NOT_CONFIGURED,
    SPI_UNSUPPORTED, SPI_TIMEOUT, SPI_BUSY, SPI_RESOURCE_CONFLICT,
    SPI_INVALID_STATE, SPI_MODE_FAULT, SPI_OVERRUN, SPI_OTHER,
};
#ifndef EMELINO_SPI_DEVICE_CAPACITY
#define EMELINO_SPI_DEVICE_CAPACITY 8
#endif

typedef struct EmelinoSpiBus EmelinoSpiBus;
typedef struct {
    int32_t (*validate_config)(EmelinoSpiBus *, EmelaSpiConfig);
    int32_t (*configure)(EmelinoSpiBus *, EmelaSpiConfig);
    /* Transfer a span of public uint16_t words, in the current configuration.
       Mark bus->recovery_required if failure leaves hardware state uncertain. */
    int32_t (*exchange)(EmelinoSpiBus *, int32_t operation, const uint16_t *data,
        uint16_t length, uint16_t *output, uint64_t deadline);
    /* Restore this configuration. NOT_CONFIGURED means no hardware was set up. */
    int32_t (*recover)(EmelinoSpiBus *, EmelaSpiConfig);
    /* slot is a stable device token; its translation into a pin owner is private
       to the port. On failed cs_configure, no new pin claim may remain. */
    int32_t (*cs_configure)(EmelinoSpiBus *, uint16_t slot, int32_t pin, int32_t level);
    int32_t (*cs_write)(EmelinoSpiBus *, uint16_t slot, int32_t pin, int32_t level);
    void (*cs_release)(EmelinoSpiBus *, uint16_t slot, int32_t pin);
    uint64_t (*now_us)(EmelinoSpiBus *);
    bool (*delay_us)(EmelinoSpiBus *, uint64_t delay, uint64_t deadline);
} EmelinoSpiOps;

/* Zero-initialize mutable state separately for each controller. The port owns
   both this object and context; no allocation is required by the common layer. */
struct EmelinoSpiBus {
    const EmelinoSpiOps *ops;
    void *context;
    bool raw_configured;
    bool recovery_required;
    EmelaSpiConfig raw_config;
    EmelaSpiConfig current_config;
};

/* Port binding. Check execution context and serialize all SPI calls, including
   access to the shared device table. Failure must not leave a lock held. */
int32_t emelino_spi_lock(void);
void emelino_spi_unlock(void);
/* Called under the lock. Each logical id identifies a distinct bus object;
   invalid ids and valid but unsupported ids must return their respective errors. */
int32_t emelino_spi_bus(EmelaSpiId id, EmelinoSpiBus **bus);
#endif
