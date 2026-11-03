// SPDX-License-Identifier: Apache-2.0
using System;
using Antmicro.Renode.Logging;
using Antmicro.Renode.Peripherals.I2C;

namespace Antmicro.Renode.Peripherals.I2C
{
    // Test target, not a replacement for the STM32 controller. Renode 1.16.1
    // requests Read(1) once and buffers its result; return a register window so
    // the original controller can supply HAL's two/three-byte receive sequence.
    public class EmelinoI2cPeer : II2CPeripheral
    {
        public EmelinoI2cPeer() { Reset(); }
        public void Reset()
        {
            registers = new byte[256];
            for(var i = 0; i < registers.Length; ++i) registers[i] = (byte)(i ^ 0x80);
            pointer = 0;
            StallReads = false;
            Writes = Reads = Finishes = 0;
        }
        public void Write(byte[] data)
        {
            Writes++;
            this.Log(LogLevel.Info, "EMELINO_I2C_WRITE {0}", BitConverter.ToString(data));
            if(data.Length == 0) return;
            var offset = 1;
            pointer = data[0];
            if(data.Length >= 3 && data[0] == 0xA0 && data[1] == 0xB1)
            {
                pointer = data[2];
                offset = 3;
            }
            else if(data.Length >= 2 && data[0] == 1)
            {
                pointer = data[1];
                offset = 2;
            }
            for(var i = offset; i < data.Length; ++i) registers[(pointer + i - offset) & 255] = data[i];
        }
        public byte[] Read(int count = 1)
        {
            Reads++;
            this.Log(LogLevel.Info, "EMELINO_I2C_READ pointer={0} requested={1} stalled={2}", pointer, count, StallReads);
            if(StallReads) return new byte[0];
            var result = new byte[16];
            for(var i = 0; i < result.Length; ++i) result[i] = registers[(pointer + i) & 255];
            return result;
        }
        public void FinishTransmission() { Finishes++; this.Log(LogLevel.Info, "EMELINO_I2C_FINISH"); }
        public bool StallReads { get; set; }
        public uint Writes { get; private set; }
        public uint Reads { get; private set; }
        public uint Finishes { get; private set; }
        private byte[] registers;
        private int pointer;
    }
}
