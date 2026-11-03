//
// Copyright (c) 2010-2023 Antmicro
// Copyright (c) 2011-2015 Realtime Embedded
//
// This file is licensed under the MIT License.
// Full license text is available in 'LICENSE.renode-MIT'.
//
// Derived from Renode infrastructure add012af003a0f620d3da52828262676f374d121.
// Test-side corrections for master ADDR/AF and the DR + shift-register receive path.
// No firmware hooks, expected sensor values, or HAL-specific memory accesses.
using System.Collections.Generic;
using System.Linq;

using Antmicro.Renode.Core;
using Antmicro.Renode.Core.Structure;
using Antmicro.Renode.Core.Structure.Registers;
using Antmicro.Renode.Logging;
using Antmicro.Renode.Peripherals.Bus;

namespace Antmicro.Renode.Peripherals.I2C
{
    [AllowedTranslations(AllowedTranslation.WordToDoubleWord)]
    public sealed class EmelinoSTM32F4I2C : SimpleContainer<II2CPeripheral>, IDoubleWordPeripheral, IBytePeripheral, IKnownSize
    {
        public EmelinoSTM32F4I2C(IMachine machine) : base(machine)
        {
            EventInterrupt = new GPIO();
            ErrorInterrupt = new GPIO();
            CreateRegisters();
            Reset();
        }

        public byte ReadByte(long offset)
        {
            if((Registers)offset == Registers.Data)
            {
                return (byte)data.Read();
            }
            else
            {
                this.LogUnhandledRead(offset);
                return 0;
            }
        }

        public void WriteByte(long offset, byte value)
        {
            if((Registers)offset == Registers.Data)
            {
                data.Write(offset, value);
            }
            else
            {
                this.LogUnhandledWrite(offset, value);
            }
        }

        public uint ReadDoubleWord(long offset)
        {
            var result = registers.Read(offset);
            if((Registers)offset == Registers.Status1)
            {
                status1Read = true;
            }
            else if((Registers)offset == Registers.Status2 && status1Read)
            {
                status1Read = false;
                if(addressSentOrMatched.Value)
                {
                    addressSentOrMatched.Value = false;
                    if(willReadOnSelectedSlave)
                    {
                        receiving = true;
                        receiveBudget = acknowledgeEnable.Value ? -1 : (acknowledgePosition.Value ? 2 : 1);
                        FillReceive();
                    }
                    Update();
                }
            }
            return result;
        }

        public void WriteDoubleWord(long offset, uint value)
        {
            registers.Write(offset, value);
        }

        public override void Reset()
        {
            state = State.Idle;
            selectedSlave = null;
            dataToTransfer = new List<byte>();
            dataToReceive = new Queue<byte>();
            status1Read = false;
            receiving = false;
            receiveBudget = 0;
            StartCount = StopCount = ReceiveCount = NackCount = 0;
            EventInterrupt.Unset();
            ErrorInterrupt.Unset();

            registers.Reset();
            data.Reset();
        }

        public GPIO EventInterrupt
        {
            get;
            private set;
        }

        public uint StartCount { get; private set; }
        public uint StopCount { get; private set; }
        public uint ReceiveCount { get; private set; }
        public uint NackCount { get; private set; }

        public GPIO ErrorInterrupt
        {
            get;
            private set;
        }

        public long Size
        {
            get
            {
                return 0x400;
            }
        }

        private void CreateRegisters()
        {
            var control1 = new DoubleWordRegister(this).WithFlag(15, writeCallback: SoftwareResetWrite, name:"SWRST").WithFlag(9, FieldMode.Read, name:"StopGen", writeCallback: StopWrite)
                .WithFlag(8, FieldMode.Read, writeCallback: StartWrite, name:"StartGen").WithFlag(0, writeCallback: PeripheralEnableWrite, name:"PeriEn");
            var control2 = new DoubleWordRegister(this).WithValueField(0, 6, name:"Freq");
            var status1 = new DoubleWordRegister(this);
            var status2 = new DoubleWordRegister(this);
            data = new DoubleWordRegister(this);

            acknowledgeEnable = control1.DefineFlagField(10, changeCallback: (oldValue, newValue) =>
            {
                // With DR and shift register occupied, clearing ACK leaves one
                // further byte to NACK: the last-three-byte receive sequence.
                if(oldValue && !newValue && receiving) receiveBudget = 1;
            });
            acknowledgePosition = control1.DefineFlagField(11);

            bufferInterruptEnable = control2.DefineFlagField(10, changeCallback: InterruptEnableChange);
            eventInterruptEnable = control2.DefineFlagField(9, changeCallback: InterruptEnableChange);
            errorInterruptEnable = control2.DefineFlagField(8, changeCallback: InterruptEnableChange);

            dataRegister = data.DefineValueField(0, 8, valueProviderCallback: _ => DataRead(), writeCallback: (_, val) => DataWrite((uint)val));

            acknowledgeFailed = status1.DefineFlagField(10, FieldMode.Read | FieldMode.WriteZeroToClear, changeCallback: (_, __) => Update());
            dataRegisterEmpty = status1.DefineFlagField(7, FieldMode.Read);
            dataRegisterNotEmpty = status1.DefineFlagField(6, FieldMode.Read, valueProviderCallback: _ => dataToReceive?.Any() ?? false);
            byteTransferFinished = status1.DefineFlagField(2, FieldMode.Read);
            addressSentOrMatched = status1.DefineFlagField(1, FieldMode.Read);
            startBit = status1.DefineFlagField(0, FieldMode.Read);

            transmitterReceiver = status2.DefineFlagField(2, FieldMode.Read);
            masterSlave = status2.DefineFlagField(0, FieldMode.Read);
            busBusy = status2.DefineFlagField(1, FieldMode.Read);

            var registerDictionary = new Dictionary<long, DoubleWordRegister>
            {
                {(long)Registers.RiseTime, DoubleWordRegister.CreateRWRegister(0x2)},
                {(long)Registers.ClockControl, DoubleWordRegister.CreateRWRegister()},
                {(long)Registers.OwnAddress1, DoubleWordRegister.CreateRWRegister()},
                {(long)Registers.OwnAddress2, DoubleWordRegister.CreateRWRegister()},
                {(long)Registers.NoiseFilter, DoubleWordRegister.CreateRWRegister()},
                {(long)Registers.Control1, control1},
                {(long)Registers.Control2, control2},
                {(long)Registers.Status1, status1},
                {(long)Registers.Status2, status2},
                {(long)Registers.Data, data},
            };
            registers = new DoubleWordRegisterCollection(this, registerDictionary);
        }

        private void InterruptEnableChange(bool oldValue, bool newValue)
        {
            Update();
        }

        private void Update()
        {
            EventInterrupt.Set(eventInterruptEnable.Value && (startBit.Value || addressSentOrMatched.Value || byteTransferFinished.Value
                || (bufferInterruptEnable.Value && (dataRegisterEmpty.Value || dataToReceive.Count != 0))));
            ErrorInterrupt.Set(errorInterruptEnable.Value && acknowledgeFailed.Value);
        }

        private uint DataRead()
        {
            var result = 0u;
            if(dataToReceive != null && dataToReceive.Any())
            {
                result = dataToReceive.Dequeue();
            }
            else
            {
                this.Log(LogLevel.Warning, "Tried to read from an empty fifo");
            }

            FillReceive();
            byteTransferFinished.Value = dataToReceive.Count >= 2;

            Update();
            return result;
        }

        private void FillReceive()
        {
            // The F4 data register and shift register hold at most two bytes.
            // Refill using Read(1), without requiring an oversized target reply.
            while(receiving && selectedSlave != null && dataToReceive.Count < 2 && receiveBudget != 0)
            {
                var next = selectedSlave.Read(1);
                if(next == null || next.Length == 0) break;
                if(next.Length != 1)
                {
                    this.Log(LogLevel.Error, "I2C target returned {0} bytes for Read(1)", next.Length);
                    break;
                }
                dataToReceive.Enqueue(next[0]);
                ++ReceiveCount;
                this.Log(LogLevel.Info, "EMELINO_I2C_RX value={0}", next[0]);
                if(receiveBudget > 0) --receiveBudget;
            }
            byteTransferFinished.Value = dataToReceive.Count >= 2;
        }

        private void DataWrite(uint newValue)
        {
            status1Read = false;
            //moved from WriteByte
            byteTransferFinished.Value = false;
            Update();

            switch(state)
            {
            case State.AwaitingAddress:
                startBit.Value = false;
                willReadOnSelectedSlave = (newValue & 1) == 1; //LSB is 1 for read and 0 for write
                var address = (int)(newValue >> 1);
                if(ChildCollection.ContainsKey(address))
                {
                    selectedSlave = ChildCollection[address];
                    this.Log(LogLevel.Info, "EMELINO_I2C_ADDRESS address={0} read={1}", address, willReadOnSelectedSlave);
                    addressSentOrMatched.Value = true; //Note: ADDR is not set after a NACK reception - from documentation

                    transmitterReceiver.Value = !willReadOnSelectedSlave; //true when transmitting

                    if(willReadOnSelectedSlave)
                    {
                        dataToReceive.Clear();
                        receiving = false; // Wait for the ADDR clear sequence.
                        dataRegisterEmpty.Value = false;
                    }
                    else
                    {
                        state = State.AwaitingData;
                        dataToTransfer = new List<byte>();

                        dataRegisterEmpty.Value = true;
                        addressSentOrMatched.Value = true;
                    }
                }
                else
                {
                    selectedSlave = null;
                    ++NackCount;
                    this.Log(LogLevel.Info, "EMELINO_I2C_NACK address={0}", address);
                    state = State.Idle;
                    acknowledgeFailed.Value = true;
                }
                Update();
                break;
            case State.AwaitingData:
                dataToTransfer.Add((byte)newValue);

                // This model has no wire-time scheduler. Apply the flags in the
                // same MMIO operation, so a later reset cannot revive queued TX.
                dataRegisterEmpty.Value = true;
                byteTransferFinished.Value = true;
                Update();
                break;
            default:
                this.Log(LogLevel.Warning, "Writing {0} to DataRegister in unsupported state {1}.", newValue, state);
                break;
            }
        }

        private void SoftwareResetWrite(bool oldValue, bool newValue)
        {
            if(newValue)
            {
                Reset();
            }
        }

        private void StopWrite(bool oldValue, bool newValue)
        {
            this.NoisyLog("Setting STOP bit to {0}", newValue);
            if(!newValue)
            {
                return;
            }
            ++StopCount;
            this.Log(LogLevel.Info, "EMELINO_I2C_STOP");

            if(selectedSlave != null && dataToTransfer != null && dataToTransfer.Count > 0)
            {
                selectedSlave.Write(dataToTransfer.ToArray());
                dataToTransfer.Clear();
                state = State.Idle;
                Update();
            }

            if(selectedSlave != null) selectedSlave.FinishTransmission();
            receiving = false;
            state = State.Idle;
            masterSlave.Value = false;
            busBusy.Value = false;
            // STOP preserves the bytes already received into DR and shift.
            byteTransferFinished.Value = dataToReceive.Count >= 2;
            dataRegisterEmpty.Value = false;
            Update();
        }

        private void StartWrite(bool oldValue, bool newValue)
        {
            if(!newValue)
            {
                return;
            }
            ++StartCount;
            this.Log(LogLevel.Info, "EMELINO_I2C_START repeated={0}", busBusy.Value);
            status1Read = false;
            receiving = false;
            dataToReceive.Clear();

            this.NoisyLog("Setting START bit to {0}", newValue);
            if(selectedSlave != null && dataToTransfer != null && dataToTransfer.Count > 0)
            {
                // repeated start condition
                selectedSlave.Write(dataToTransfer.ToArray());
                dataToTransfer.Clear();
            }
            //TODO: TRA cleared on repeated Start condition. Is this always here?
            transmitterReceiver.Value = false;
            dataRegisterEmpty.Value = false;
            byteTransferFinished.Value = false;
            startBit.Value = true;
            if(newValue)
            {
                switch(state)
                {
                case State.Idle:
                case State.AwaitingData: //HACK! Should not be here, forced by ExecuteIn somehow.
                    state = State.AwaitingAddress;
                    masterSlave.Value = true;
                    busBusy.Value = true;
                    Update();
                    break;
                }
            }
        }

        private void PeripheralEnableWrite(bool oldValue, bool newValue)
        {
            if(!newValue)
            {
                receiving = false;
                status1Read = false;
                dataToReceive.Clear();
                dataToTransfer.Clear();
                selectedSlave = null;
                state = State.Idle;
                acknowledgeEnable.Value = false;
                masterSlave.Value = false;
                busBusy.Value = false;
                acknowledgeFailed.Value = false;
                transmitterReceiver.Value = false;
                dataRegisterEmpty.Value = false;
                byteTransferFinished.Value = false;
                addressSentOrMatched.Value = false;
                startBit.Value = false;
                Update();
            }
        }

        private DoubleWordRegister data;
        private IFlagRegisterField acknowledgeEnable, acknowledgePosition, busBusy;
        private IFlagRegisterField bufferInterruptEnable, eventInterruptEnable, errorInterruptEnable;
        private IValueRegisterField dataRegister;
        private IFlagRegisterField acknowledgeFailed, dataRegisterEmpty, dataRegisterNotEmpty, byteTransferFinished, addressSentOrMatched, startBit;
        private IFlagRegisterField transmitterReceiver, masterSlave;

        private DoubleWordRegisterCollection registers;

        private State state;
        private List<byte> dataToTransfer;
        private Queue<byte> dataToReceive;
        private bool willReadOnSelectedSlave;
        private bool status1Read, receiving;
        private int receiveBudget;
        private II2CPeripheral selectedSlave;

        private enum Registers
        {
            Control1 = 0x0,
            Control2 = 0x4,
            OwnAddress1 = 0x8,
            OwnAddress2 = 0xC,
            Data = 0x10,
            Status1 = 0x14,
            Status2 = 0x18,
            ClockControl = 0x1C,
            RiseTime = 0x20,
            NoiseFilter = 0x24,
        }

        private enum State
        {
            Idle,
            AwaitingAddress,
            AwaitingData,
        }
    }
}
