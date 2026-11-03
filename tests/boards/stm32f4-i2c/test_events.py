#!/usr/bin/env python3
"""Exercise the production STOP deferral with fake HAL registers and IRQ delivery."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]


class I2cEventsTest(unittest.TestCase):
    def test_stop_wait_yields_without_reading_data_or_disabling_errors(self):
        harness = r'''
#include <assert.h>
#include <stdbool.h>
#include <stdint.h>
enum { HAL_I2C_STATE_BUSY_RX = 1, HAL_I2C_STATE_READY = 2,
       I2C_CR1_STOP = 0x200, I2C_IT_EVT = 0x200, I2C_IT_BUF = 0x400, I2C_IT_ERR = 0x100 };
typedef struct { uint32_t CR1, CR2; } Registers;
typedef struct { Registers *Instance; uint32_t State; uint16_t XferCount; } I2C_HandleTypeDef;
static uint32_t masked, calls, data_reads;
static uint32_t stm32f4_interrupt_lock(void) { uint32_t old = masked; masked = 1; return old; }
static void stm32f4_interrupt_unlock(uint32_t old) { masked = old; }
#define __HAL_I2C_DISABLE_IT(handle, bits) ((handle)->Instance->CR2 &= ~(bits))
#define __HAL_I2C_ENABLE_IT(handle, bits) ((handle)->Instance->CR2 |= (bits))
static void HAL_I2C_EV_IRQHandler(I2C_HandleTypeDef *handle) {
    calls++;
    if (handle->State == HAL_I2C_STATE_BUSY_RX && handle->XferCount == 1) {
        /* The real HAL waits here, then reads DR. Never enter while STOP is
           asserted: main must retain control of the whole-transfer deadline. */
        assert(!(handle->Instance->CR1 & I2C_CR1_STOP));
        data_reads++;
        handle->XferCount = 0;
        handle->State = HAL_I2C_STATE_READY;
    }
}
#include "i2c_events.h"
int main(void) {
    Registers registers = {I2C_CR1_STOP, I2C_IT_EVT | I2C_IT_BUF | I2C_IT_ERR};
    I2C_HandleTypeDef handle = {&registers, HAL_I2C_STATE_BUSY_RX, 1};
    volatile bool deferred = false;
    stm32f4_i2c_event(&handle, &deferred);
    assert(deferred && calls == 0 && data_reads == 0);
    assert(registers.CR2 == I2C_IT_ERR); /* Errors remain deliverable. */
    for (unsigned i = 0; i < 10; i++) stm32f4_i2c_resume_event(&handle, &deferred);
    assert(deferred && registers.CR2 == I2C_IT_ERR && !masked);
    registers.CR1 = 0;
    stm32f4_i2c_resume_event(&handle, &deferred);
    assert(!deferred && registers.CR2 == (I2C_IT_EVT | I2C_IT_BUF | I2C_IT_ERR));
    stm32f4_i2c_event(&handle, &deferred);
    assert(calls == 1 && data_reads == 1 && handle.State == HAL_I2C_STATE_READY);
    /* A bus error can complete the transfer while STOP is deferred. */
    deferred = true;
    registers.CR2 = 0;
    masked = 1;
    stm32f4_i2c_resume_event(&handle, &deferred);
    assert(!deferred && registers.CR2 == 0 && masked == 1);
    /* Non-final receive phases and other HAL events are not suppressed. */
    masked = 0;
    handle.State = HAL_I2C_STATE_BUSY_RX;
    handle.XferCount = 2;
    registers.CR1 = I2C_CR1_STOP;
    stm32f4_i2c_event(&handle, &deferred);
    assert(calls == 2 && !deferred && data_reads == 1);
}
'''
        with tempfile.TemporaryDirectory(prefix="emelino-i2c-events-") as temporary:
            directory = Path(temporary)
            source = directory / "test.c"
            source.write_text(harness)
            executable = directory / "test"
            subprocess.run([os.environ.get("CC", "cc"), "-std=c11", "-Wall", "-Wextra", "-Werror",
                            "-I", str(ROOT / "firmware/ports/stm32f4"), str(source), "-o", str(executable)], check=True)
            subprocess.run([str(executable)], check=True)


if __name__ == "__main__":
    unittest.main()
