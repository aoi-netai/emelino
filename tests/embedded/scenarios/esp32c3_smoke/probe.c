#include <stdint.h>
volatile uint32_t esp32c3_test_done, esp32c3_test_checks, esp32c3_test_failures, esp32c3_test_last_failed;
void esp32c3_test_check(int64_t number, int64_t success) {
    ++esp32c3_test_checks;
    if (!success) { ++esp32c3_test_failures; esp32c3_test_last_failed = (uint32_t)number; }
}
void esp32c3_test_finish(void) { esp32c3_test_done = 1; }
