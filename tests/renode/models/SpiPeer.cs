// SPDX-License-Identifier: Apache-2.0
using Antmicro.Renode.Core;
using Antmicro.Renode.Logging;

namespace Antmicro.Renode.Peripherals.SPI
{
    // A deterministic SPI slave. GPIO CS observations are independent of the bus.
    public class EmelinoSpiPeer : ISPIPeripheral, IGPIOReceiver
    {
        public void Reset() { }

        public byte Transmit(byte value)
        {
            var response = (byte)(value ^ 0xa5);
            this.Log(LogLevel.Info, "EMELINO_SPI tx={0} rx={1}", value, response);
            return response;
        }

        public void FinishTransmission() { }

        public void OnGPIO(int number, bool value)
        {
            this.Log(LogLevel.Info, "EMELINO_SPI_CS pin={0} level={1}", number, value ? 1 : 0);
        }
    }
}
