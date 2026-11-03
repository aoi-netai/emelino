#!/usr/bin/env python3
"""Exercise the production wait loop with deterministic monotonic clock samples."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]


class DeadlineTest(unittest.TestCase):
    def test_completion_cannot_skip_deadline_after_interrupt_delay(self):
        # Compile the actual wait function separately from the Cortex-M clock.
        # Clock samples model normal progress and preemption across a deadline.
        source = (ROOT / "firmware/ports/stm32f4/time.c").read_text()
        start = source.index("bool stm32f4_delay_us(")
        first_brace = source.index("{", start)
        depth = 1
        end = first_brace + 1
        while depth:
            depth += (source[end] == "{") - (source[end] == "}")
            end += 1
        function = source[start:end]
        harness = r'''
#include <assert.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
static const uint64_t *samples;
static size_t sample_count, position;
uint64_t stm32f4_now_us(void) {
    assert(position < sample_count);
    return samples[position++];
}
''' + function + r'''
static void check(const uint64_t *clock, size_t count,
                  uint64_t delay, uint64_t deadline, bool expected) {
    samples = clock;
    sample_count = count;
    position = 0;
    assert(stm32f4_delay_us(delay, deadline) == expected);
}
int main(void) {
    const uint64_t normal[] = {0, 1, 4, 5};
    check(normal, 4, 5, 10, true);
    const uint64_t preempted[] = {0, 20};
    check(preempted, 2, 5, 10, false);
    const uint64_t exact_deadline[] = {0, 10};
    check(exact_deadline, 2, 5, 10, false);
    const uint64_t already_expired[] = {10};
    check(already_expired, 1, 0, 10, false);
    const uint64_t zero_delay[] = {2, 2};
    check(zero_delay, 2, 0, 10, true);
    const uint64_t impossible[] = {8};
    check(impossible, 1, 3, 10, false);
    const uint64_t boundary[] = {UINT64_MAX - 2, UINT64_MAX - 1};
    check(boundary, 2, 1, UINT64_MAX, true);
}
'''
        with tempfile.TemporaryDirectory(prefix="emelino-time-") as temporary:
            directory = Path(temporary)
            path = directory / "test.c"
            path.write_text(harness)
            executable = directory / "test"
            subprocess.run([os.environ.get("CC", "cc"), "-std=c11", "-Wall", "-Wextra",
                            "-Werror", str(path), "-o", str(executable)], check=True)
            subprocess.run([str(executable)], check=True)


if __name__ == "__main__":
    unittest.main()
