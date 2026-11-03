#include "../../support/probe.h"

/* Observations only: no HAL wrappers or peripheral-register overrides. */
EMELINO_TEST_PROBE(bmp180_test)
volatile uint32_t bmp180_test_current;
volatile uint32_t bmp180_test_error;
volatile int32_t bmp180_test_temperature[2];

void bmp180_test_mark(int32_t number) { bmp180_test_current = (uint32_t)number; }
void bmp180_test_error_record(int32_t kind) {
    bmp180_test_error = (uint32_t)kind;
    bmp180_test_check((int32_t)bmp180_test_current, 0);
}
void bmp180_test_temperature_record(int32_t slot, int32_t value) {
    if (slot >= 0 && slot < 2) bmp180_test_temperature[slot] = value;
}
