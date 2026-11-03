// SPDX-License-Identifier: Apache-2.0
using System;
using Antmicro.Renode.Core;
using Antmicro.Renode.Logging;
using Antmicro.Renode.Peripherals;
using Antmicro.Renode.Peripherals.Bus;
using Antmicro.Renode.Peripherals.I2C;

namespace Antmicro.Renode.Peripherals.I2C
{
    // Strict Read(count) target for controller-only register tests. The target
    // has no knowledge of the controller's registers, firmware, or HAL state.
    public sealed class EmelinoCountingPeer : II2CPeripheral
    {
        public EmelinoCountingPeer() { Reset(); }
        public void Reset()
        {
            registers = new byte[256];
            for(var i = 0; i < registers.Length; ++i) registers[i] = (byte)(i ^ 0x80);
            Pointer = ReadCalls = WriteCalls = Finishes = ReadBytes = LastReadCount = LastWriteSize = 0;
        }
        public void Write(byte[] bytes)
        {
            ++WriteCalls;
            LastWriteSize = (uint)bytes.Length;
            if(bytes.Length == 0) return;
            Pointer = bytes[0];
            for(var i = 1; i < bytes.Length; ++i)
            {
                registers[Pointer] = bytes[i];
                Pointer = (Pointer + 1) & 255;
            }
        }
        public byte[] Read(int count = 1)
        {
            if(count < 0) throw new ArgumentOutOfRangeException("count");
            ++ReadCalls;
            LastReadCount = (uint)count;
            ReadBytes += (uint)count;
            var result = new byte[count];
            for(var i = 0; i < count; ++i)
            {
                result[i] = registers[Pointer];
                Pointer = (Pointer + 1) & 255;
            }
            return result;
        }
        public void FinishTransmission() { ++Finishes; }
        public uint ReadCalls { get; private set; }
        public uint WriteCalls { get; private set; }
        public uint Finishes { get; private set; }
        public uint ReadBytes { get; private set; }
        public uint LastReadCount { get; private set; }
        public uint LastWriteSize { get; private set; }
        public uint Pointer { get; private set; }
        private byte[] registers;
    }
}

namespace Antmicro.Renode.Peripherals.Miscellaneous
{
    // Expose target counters and actual GPIO IRQ levels through independent
    // MMIO, so the runner never reads controller-private implementation state.
    public sealed class EmelinoI2cInspector : IDoubleWordPeripheral, IKnownSize, IGPIOReceiver
    {
        public EmelinoI2cInspector(EmelinoCountingPeer peer) { this.peer = peer; }
        public void Reset() { levels = 0; }
        public void OnGPIO(int number, bool value)
        {
            if(number < 0 || number > 1) throw new ArgumentOutOfRangeException("number");
            if(value) levels |= 1U << number;
            else levels &= ~(1U << number);
        }
        public uint ReadDoubleWord(long offset)
        {
            switch(offset)
            {
            case 0: return levels;
            case 4: return peer.ReadCalls;
            case 8: return peer.WriteCalls;
            case 12: return peer.Finishes;
            case 16: return peer.ReadBytes;
            case 20: return peer.LastReadCount;
            case 24: return peer.Pointer;
            case 28: return peer.LastWriteSize;
            default: this.LogUnhandledRead(offset); return 0;
            }
        }
        public void WriteDoubleWord(long offset, uint value) { this.LogUnhandledWrite(offset, value); }
        public long Size { get { return 0x100; } }
        private readonly EmelinoCountingPeer peer;
        private uint levels;
    }
}
