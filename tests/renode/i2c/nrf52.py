"""Legacy nRF52840 TWI register contract, independent of the Emelino BSP."""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
MODEL = ROOT / "tests/renode/models/EmelinoNRF52840I2C.cs"
PLATFORM = ('i2c: I2C.EmelinoNRF52840I2C @ sysbus 0x40003000\n'
            '    IRQ -> inspect@0\n'
            'peer: I2C.EmelinoCountingPeer @ i2c 0x50\n'
            'inspect: Miscellaneous.EmelinoI2cInspector @ sysbus 0x50000000\n    peer: peer\n')
BASE, INSPECT = 0x40003000, 0x50000000
STARTRX, STARTTX, STOP, SUSPEND, RESUME = 0, 8, 0x14, 0x1C, 0x20
STOPPED, RXREADY, TXSENT, ERROR, BB, SUSPENDED = 0x104, 0x108, 0x11C, 0x124, 0x138, 0x148
SHORTS, INTSET, INTCLR, ERRORSRC, ENABLE, RXD, TXD, ADDRESS = 0x200, 0x304, 0x308, 0x4C4, 0x500, 0x518, 0x51C, 0x588


def scenario(script, checks):
    def write(offset, number):
        script.append(f'sysbus WriteDoubleWord {BASE + offset:#x} {number:#x}')

    def read(name, offset, expected, inspector=False, mask=0xFFFFFFFF):
        label = "CHECK_" + name
        script.extend([f'echo "{label}"', f'sysbus ReadDoubleWord {(INSPECT if inspector else BASE) + offset:#x}'])
        checks.append(dict(name=name, label=label, expected=expected, mask=mask))

    def reset(mask=0):
        script.extend(['sysbus.i2c Reset', 'sysbus.i2c.peer Reset', 'sysbus.inspect Reset'])
        write(ENABLE, 5)
        write(ADDRESS, 0x50)
        write(INTSET, mask)

    # A START/STOP without TXD probes the address without modifying the device.
    reset(2)
    write(STARTTX, 1)
    read('probe_no_error', ERROR, 0)
    read('probe_no_data', 8, 0, True)
    write(STOP, 1)
    read('probe_stopped', STOPPED, 1)
    read('probe_finished_once', 12, 1, True)
    read('probe_irq', 0, 1, True)
    write(STOPPED, 0)
    read('probe_irq_ack', 0, 0, True)
    write(STOP, 1)
    read('idle_stop_not_duplicated', 12, 1, True)

    # BB_STOP ends each one-byte read before software consumes its RXD.
    reset(6)
    write(SHORTS, 2)
    for i in range(2):
        write(STOPPED, 0)
        write(RXREADY, 0)
        write(STARTRX, 1)
        read(f'single_{i}_ready', RXREADY, 1)
        read(f'single_{i}_stopped', STOPPED, 1)
        read(f'single_{i}_finished_before_rxd', 12, i + 1, True)
        read(f'single_{i}_data', RXD, 0x80 + i)
        read(f'single_{i}_no_prefetch', 16, i + 1, True)
        read(f'single_{i}_read_count', 20, 1, True)
        write(RXREADY, 0)
        write(STOPPED, 0)
        read(f'single_{i}_irq_ack', 0, 0, True)

    # BB_SUSPEND keeps the bus active without requesting the next byte.
    for size in (2, 8):
        reset(4)
        write(SHORTS, 1)
        write(STARTRX, 1)
        for i in range(size):
            read(f'rx_{size}_{i}_data', RXD, 0x80 + i)
            read(f'rx_{size}_{i}_byte_count', 16, i + 1, True)
            write(RXREADY, 0)
            read(f'rx_{size}_{i}_irq_cleared', 0, 0, True)
            if i < size - 1:
                read(f'rx_{size}_{i}_held', SUSPENDED, 1)
                read(f'rx_{size}_{i}_no_stop', 12, 0, True)
                write(SUSPENDED, 0)
                write(SHORTS, 2 if i == size - 2 else 1)
                write(RESUME, 1)
        read(f'rx_{size}_final_stop', STOPPED, 1)
        read(f'rx_{size}_one_finish', 12, 1, True)
        write(RESUME, 1)
        read(f'rx_{size}_resume_after_stop_ignored', 16, size, True)

    # Without SHORTS, acknowledge/read order must not stall the next byte.
    for clear_first in (True, False):
        name = "continuous_" + str(clear_first)
        reset(4)
        write(STARTRX, 1)
        if clear_first:
            write(RXREADY, 0)
            read(name + "_no_early_fetch", 16, 1, True)
        read(name + "_first", RXD, 0x80)
        if not clear_first:
            read(name + "_no_early_fetch", 16, 1, True)
            write(RXREADY, 0)
        read(name + "_next_ready", RXREADY, 1)
        read(name + "_two_bytes", 16, 2, True)
        write(STOP, 1)
        read(name + "_last_survives_stop", RXD, 0x81)
        write(RXREADY, 0)
        read(name + "_no_extra_fetch", 16, 2, True)

    # Repeated START delivers the prefix but does not finish the transaction.
    reset()
    write(STARTTX, 1)
    write(TXD, 0x30)
    read('prefix_txd_sent', TXSENT, 1)
    write(SUSPEND, 1)
    read('prefix_delivered', 8, 1, True)
    read('prefix_no_stop', 12, 0, True)
    write(SHORTS, 2)
    write(STARTRX, 1)
    read('prefix_read_correct_register', RXD, 0xB0)
    read('prefix_one_read', 4, 1, True)
    read('prefix_one_stop', 12, 1, True)

    # ANACK is latched/W1C independently of EVENTS_ERROR and IRQ masking.
    reset(512)
    write(ADDRESS, 0x51)
    write(STARTTX, 1)
    read('nack_error', ERROR, 1)
    read('nack_source_first', ERRORSRC, 2)
    read('nack_source_second', ERRORSRC, 2)
    read('nack_irq', 0, 1, True)
    write(INTCLR, 512)
    read('nack_masked', 0, 0, True)
    write(INTSET, 512)
    read('nack_unmasked', 0, 1, True)
    write(ERRORSRC, 0)
    read('nack_write_zero_preserves', ERRORSRC, 2)
    write(ERRORSRC, 2)
    read('nack_w1c', ERRORSRC, 0)
    read('nack_event_separate', ERROR, 1)
    write(ERROR, 0)
    read('nack_event_ack', 0, 0, True)
    write(STOP, 1)
    read('nack_stop', STOPPED, 1)
    read('nack_no_peer_finish', 12, 0, True)

    # Disabled or reset controllers must not perform bus operations.
    reset(4)
    write(ENABLE, 0)
    write(STARTRX, 1)
    write(TXD, 42)
    write(STOP, 1)
    read('disabled_no_reads', 4, 0, True)
    read('disabled_no_writes', 8, 0, True)
    read('disabled_no_stop', STOPPED, 0)
    write(ENABLE, 5)
    write(SHORTS, 1)
    write(STARTRX, 1)
    read('reset_before_irq', 0, 1, True)
    write(ENABLE, 0)
    read('disable_preserves_pending_irq', 0, 1, True)
    read('disable_preserves_event', RXREADY, 1)
    write(INTCLR, 4)
    read('disable_mask_deasserts_irq', 0, 0, True)
    write(INTSET, 4)
    read('disable_unmask_asserts_irq', 0, 1, True)
    script.append('sysbus.i2c Reset')
    read('reset_after_irq', 0, 0, True)
    read('reset_events', RXREADY, 0)
    write(RESUME, 1)
    read('reset_no_extra_reads', 4, 1, True)
