#include "../../support/probe.h"
EMELINO_TEST_PROBE(callbacks)
volatile uint32_t callbacks_event_count;
volatile uint32_t callbacks_events[32];
volatile uint32_t callbacks_ipsr[32];
int32_t callbacks_seed(void) { return 20; }
void callbacks_record(int32_t tag, int32_t pin, int32_t level) {
    uint32_t index = callbacks_event_count;
    uint32_t ipsr;
    __asm volatile("mrs %0, ipsr" : "=r"(ipsr));
    if (index < 32) {
        callbacks_events[index] = (uint32_t)(tag * 10000 + pin * 10 + level);
        callbacks_ipsr[index] = ipsr;
    }
    callbacks_event_count = index + 1;
}
