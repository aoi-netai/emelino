// Copyright (c) 2010-2026 Antmicro
// Modifications Copyright (c) 2026 Emelino contributors
// SPDX-License-Identifier: MIT
// Derived from NRF52840_I2C.cs at renode-infrastructure 76fa7d720e52377ae1986c22fae84965c2aa72e9.
// Full license: LICENSE.renode-MIT in this directory.

using System;
using System.Collections.Generic;
using Antmicro.Renode.Core;
using Antmicro.Renode.Core.Structure;
using Antmicro.Renode.Core.Structure.Registers;
using Antmicro.Renode.Logging;
using Antmicro.Renode.Peripherals.Bus;
using Antmicro.Renode.Peripherals.Miscellaneous;

namespace Antmicro.Renode.Peripherals.I2C
{
    // Legacy TWI master only. Data moves at register-operation boundaries; bus
    // timing and electrical arbitration are deliberately outside this model.
    public class EmelinoNRF52840I2C : SimpleContainer<II2CPeripheral>,
        IProvidesRegisterCollection<DoubleWordRegisterCollection>, IDoubleWordPeripheral,
        IKnownSize, INRFEventProvider
    {
        public EmelinoNRF52840I2C(IMachine machine) : base(machine)
        {
            IRQ = new GPIO();
            RegistersCollection = new DoubleWordRegisterCollection(this);
            DefineRegisters();
            Reset();
        }

        public override void Reset()
        {
            RegistersCollection.Reset();
            enabled = active = receiving = suspended = rxValid = false;
            events = interrupts = 0;
            rxData = 0;
            selectedSlave = null;
            transmitted.Clear();
            UpdateInterrupts();
        }

        public uint ReadDoubleWord(long offset) { return RegistersCollection.Read(offset); }
        public void WriteDoubleWord(long offset, uint value) { RegistersCollection.Write(offset, value); }
        public DoubleWordRegisterCollection RegistersCollection { get; private set; }
        public GPIO IRQ { get; private set; }
        public long Size { get { return 0x1000; } }
        public event Action<uint> EventTriggered;

        private void DefineRegisters()
        {
            Registers.StartReceiving.Define(this)
                .WithFlag(0, FieldMode.Write, name: "TASKS_STARTRX", writeCallback: (_, value) => { if(value) Start(true); })
                .WithReservedBits(1, 31);
            Registers.StartTransmitting.Define(this)
                .WithFlag(0, FieldMode.Write, name: "TASKS_STARTTX", writeCallback: (_, value) => { if(value) Start(false); })
                .WithReservedBits(1, 31);
            Registers.Stop.Define(this)
                .WithFlag(0, FieldMode.Write, name: "TASKS_STOP", writeCallback: (_, value) => { if(value && enabled) Stop(); })
                .WithReservedBits(1, 31);
            Registers.Suspend.Define(this)
                .WithFlag(0, FieldMode.Write, name: "TASKS_SUSPEND", writeCallback: (_, value) => { if(value && enabled && active) Suspend(); })
                .WithReservedBits(1, 31);
            Registers.Resume.Define(this)
                .WithFlag(0, FieldMode.Write, name: "TASKS_RESUME", writeCallback: (_, value) =>
                {
                    if(value && enabled && active && suspended)
                    {
                        suspended = false;
                        if(receiving) ReceiveByte();
                    }
                }).WithReservedBits(1, 31);

            DefineEvent(Registers.Stopped, Stopped, "EVENTS_STOPPED");
            DefineEvent(Registers.RxReady, RxReady, "EVENTS_RXDREADY");
            DefineEvent(Registers.TxSent, TxSent, "EVENTS_TXDSENT");
            DefineEvent(Registers.Error, Error, "EVENTS_ERROR");
            DefineEvent(Registers.ByteBoundary, ByteBoundary, "EVENTS_BB");
            DefineEvent(Registers.Suspended, Suspended, "EVENTS_SUSPENDED");

            Registers.Shortcuts.Define(this)
                .WithFlag(0, out suspendShortcut, name: "BB_SUSPEND")
                .WithFlag(1, out stopShortcut, name: "BB_STOP")
                .WithReservedBits(2, 30);
            Registers.InterruptSet.Define(this)
                .WithValueField(0, 32, name: "INTENSET", valueProviderCallback: _ => interrupts,
                    writeCallback: (_, value) => { interrupts |= (uint)value & InterruptMask; UpdateInterrupts(); });
            Registers.InterruptClear.Define(this)
                .WithValueField(0, 32, name: "INTENCLR", valueProviderCallback: _ => interrupts,
                    writeCallback: (_, value) => { interrupts &= ~(uint)value; UpdateInterrupts(); });
            Registers.ErrorSource.Define(this)
                .WithValueField(0, 3, out errors, FieldMode.Read | FieldMode.WriteOneToClear, name: "ERRORSRC")
                .WithReservedBits(3, 29);
            Registers.Enable.Define(this)
                .WithValueField(0, 4, name: "ENABLE", writeCallback: (_, value) =>
                {
                    enabled = value == 5;
                    if(!enabled)
                    {
                        // Disabling is an abort, not a fabricated STOP on the wire.
                        active = receiving = suspended = rxValid = false;
                        selectedSlave = null;
                        transmitted.Clear();
                    }
                    UpdateInterrupts();
                }).WithReservedBits(4, 28);
            Registers.PinSCL.Define(this, 0xFFFFFFFF).WithValueField(0, 32, name: "PSEL.SCL");
            Registers.PinSDA.Define(this, 0xFFFFFFFF).WithValueField(0, 32, name: "PSEL.SDA");
            Registers.Frequency.Define(this).WithValueField(0, 32, name: "FREQUENCY");
            Registers.Address.Define(this).WithValueField(0, 7, out address, name: "ADDRESS").WithReservedBits(7, 25);
            Registers.ReceiveBuffer.Define(this)
                .WithValueField(0, 8, FieldMode.Read, name: "RXD", valueProviderCallback: _ =>
                {
                    var result = rxData;
                    rxValid = false;
                    ContinueReceiving();
                    return result;
                }).WithReservedBits(8, 24);
            Registers.TransmitBuffer.Define(this)
                .WithValueField(0, 8, name: "TXD", writeCallback: (_, value) =>
                {
                    if(!enabled || !active || receiving || suspended || selectedSlave == null) return;
                    transmitted.Add((byte)value);
                    Raise(Registers.TxSent, TxSent);
                    Boundary();
                }).WithReservedBits(8, 24);
        }

        private void DefineEvent(Registers register, uint mask, string name)
        {
            register.Define(this).WithFlag(0, name: name,
                valueProviderCallback: _ => (events & mask) != 0,
                writeCallback: (_, value) =>
                {
                    if(value) events |= mask;
                    else events &= ~mask;
                    // With no BB shortcut, the receiver runs continuously. The
                    // next byte becomes observable after software consumes RXD
                    // and acknowledges RXDREADY. No bulk Read() can over-fetch.
                    if(mask == RxReady && !value) ContinueReceiving();
                    UpdateInterrupts();
                }).WithReservedBits(1, 31);
        }

        private void Start(bool read)
        {
            if(!enabled) return;
            FlushWrite();
            active = true;
            receiving = read;
            suspended = rxValid = false;
            this.Log(LogLevel.Info, "EMELINO_I2C_START");
            if(!TryGetByAddress((int)address.Value, out selectedSlave))
            {
                errors.Value |= 2; // ANACK
                this.Log(LogLevel.Info, "EMELINO_I2C_NACK address={0}", address.Value);
                Raise(Registers.Error, Error);
                return;
            }
            if(read) ReceiveByte();
        }

        private void ReceiveByte()
        {
            if(!enabled || !active || !receiving || suspended || selectedSlave == null || rxValid) return;
            var data = selectedSlave.Read(1);
            if(data == null || data.Length == 0) return;
            if(data.Length != 1)
                this.Log(LogLevel.Warning, "I2C peer returned {0} bytes for a one-byte request", data.Length);
            rxData = data[0];
            rxValid = true;
            Raise(Registers.RxReady, RxReady);
            Boundary();
        }

        private void ContinueReceiving()
        {
            // Both clear-before-read and read-before-clear acknowledge a byte.
            if((events & RxReady) == 0 && !rxValid) ReceiveByte();
        }

        private void Boundary()
        {
            Raise(Registers.ByteBoundary, ByteBoundary);
            // Normal software enables exactly one of these two shortcuts.
            if(stopShortcut.Value) Stop();
            else if(suspendShortcut.Value) Suspend();
        }

        private void Suspend()
        {
            FlushWrite();
            suspended = true;
            Raise(Registers.Suspended, Suspended);
        }

        private void FlushWrite()
        {
            if(transmitted.Count == 0 || selectedSlave == null) return;
            selectedSlave.Write(transmitted.ToArray());
            transmitted.Clear();
        }

        private void Stop()
        {
            if(!active) return;
            FlushWrite();
            selectedSlave?.FinishTransmission();
            active = receiving = suspended = false;
            // RXD remains readable when BB_STOP has already ended the transfer.
            this.Log(LogLevel.Info, "EMELINO_I2C_STOP");
            Raise(Registers.Stopped, Stopped);
        }

        private void Raise(Registers register, uint mask)
        {
            events |= mask;
            EventTriggered?.Invoke((uint)register);
            UpdateInterrupts();
        }
        // ENABLE stops transfers; it does not acknowledge already latched events.
        private void UpdateInterrupts() { IRQ.Set((events & interrupts) != 0); }

        private readonly List<byte> transmitted = new List<byte>();
        private II2CPeripheral selectedSlave;
        private bool enabled, active, receiving, suspended, rxValid;
        private byte rxData;
        private uint events, interrupts;
        private IValueRegisterField address, errors;
        private IFlagRegisterField suspendShortcut, stopShortcut;
        private const uint Stopped = 1u << 1, RxReady = 1u << 2, TxSent = 1u << 7,
            Error = 1u << 9, ByteBoundary = 1u << 14, Suspended = 1u << 18;
        private const uint InterruptMask = Stopped | RxReady | TxSent | Error | ByteBoundary | Suspended;
        private enum Registers
        {
            StartReceiving = 0x000, StartTransmitting = 0x008, Stop = 0x014,
            Suspend = 0x01C, Resume = 0x020, Stopped = 0x104, RxReady = 0x108,
            TxSent = 0x11C, Error = 0x124, ByteBoundary = 0x138, Suspended = 0x148,
            Shortcuts = 0x200, InterruptSet = 0x304, InterruptClear = 0x308,
            ErrorSource = 0x4C4, Enable = 0x500, PinSCL = 0x508, PinSDA = 0x50C,
            ReceiveBuffer = 0x518, TransmitBuffer = 0x51C, Frequency = 0x524, Address = 0x588,
        }
    }
}
