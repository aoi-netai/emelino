#!/usr/bin/env python3
"""Check STM32F4 I2C model register behavior without firmware or HAL."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
from test_stm32f4_gpio import run, value

ROOT = Path(__file__).resolve().parents[1]
MODEL = ROOT / "tests/renode/models/EmelinoSTM32F4I2C.cs"
PEER = ROOT / "tests/renode/i2c/CountingPeer.cs"
BASE, INSPECT = 0x40005400, 0x50000000
CR1, CR2, DR, SR1, SR2 = 0x00, 0x04, 0x10, 0x14, 0x18
PE, START, STOP, ACK, POS = 1, 0x100, 0x200, 0x400, 0x800
EVT, BUF, ERR = 0x200, 0x400, 0x100


def scenario(script, checks):
    def write(offset, data):
        script.append(f"sysbus WriteDoubleWord 0x{BASE + offset:x} 0x{data:x}")

    def read(name, offset, expected, mask=0xffffffff, inspector=False, byte=False):
        label = "CHECK_" + name
        address = (INSPECT if inspector else BASE) + offset
        script.extend([f'echo "{label}"', f'sysbus Read{"Byte" if byte else "DoubleWord"} 0x{address:x}'])
        checks.append({"name": name, "label": label, "expected": expected, "mask": mask})

    def step():
        # Allow the installed Renode scheduler to deliver any queued IRQ update.
        script.append('emulation RunFor "0:00:00.000001"')

    def reset(interrupts=0):
        script.extend(["sysbus.i2c Reset", "sysbus.i2c.peer Reset"])
        write(CR1, PE)
        write(CR2, 16 | interrupts)
        step()

    def address(name, receive=True, ack=False):
        write(CR1, PE | START | (ACK if ack else 0))
        step()
        read(name + "_start", SR1, 1, 1)
        write(DR, 0xa1 if receive else 0xa0)
        step()

    def clear_address(name, receive=True):
        read(name + "_addr", SR1, 2, 2)
        read(name + "_master_busy", SR2, 3 | (0 if receive else 4), 7)

    # SR2 alone must not dismiss a newly received address. SR1 -> SR2 does.
    reset()
    address("addr")
    read("addr_sr2_alone", SR2, 3, 7)
    read("addr_still_set", SR1, 2, 2)
    read("addr_sr2_sequence", SR2, 3, 7)
    read("addr_cleared", SR1, 0, 2)
    read("addr_only_one_byte", 4, 1, inspector=True)
    read("addr_count_honored", 20, 1, inspector=True)
    write(CR1, PE | STOP)
    read("addr_first_data", DR, 0x80)

    # One-byte final receive: STOP can complete before DR is consumed.
    reset(EVT | BUF | ERR)
    address("single")
    clear_address("single")
    write(CR1, PE | STOP)
    step()
    read("single_flags", SR1, 0x40, 0x44)
    read("single_stop_cleared", CR1, 0, STOP)
    read("single_idle", SR2, 0, 3)
    read("single_irq", 0, 1, inspector=True)
    read("single_data", DR, 0x80, byte=True)
    read("single_drained", SR1, 0, 0x44)
    read("single_irq_cleared", 0, 0, inspector=True)
    read("single_reads", 4, 1, inspector=True)
    read("single_finish", 12, 1, inspector=True)

    # Two-byte final receive: POS shifts NACK to the second incoming byte.
    reset()
    address("double")
    write(CR1, PE | POS)
    clear_address("double")
    read("double_flags", SR1, 0x44, 0x44)
    write(CR1, PE | POS | STOP)
    read("double_data0", DR, 0x80)
    read("double_one_left", SR1, 0x40, 0x44)
    read("double_data1", DR, 0x81)
    read("double_drained", SR1, 0, 0x44)
    read("double_reads", 4, 2, inspector=True)
    read("double_finish", 12, 1, inspector=True)

    # RM0390 final-three-byte sequence also terminates a long transfer.
    for length in (3, 8):
        name = f"receive_{length}"
        reset()
        address(name, ack=True)
        clear_address(name)
        read(name + "_first_btf", SR1, 0x44, 0x44)
        for index in range(length - 3):
            read(name + f"_prefix_{index}", DR, 0x80 + index)
        write(CR1, PE)  # ACK falls with DR + shift register occupied.
        read(name + "_before_final_three", 4, length - 1, inspector=True)
        read(name + "_third_last", DR, 0x80 + length - 3)
        read(name + "_last_btf", SR1, 0x44, 0x44)
        write(CR1, PE | STOP)
        read(name + "_second_last", DR, 0x80 + length - 2)
        read(name + "_last", DR, 0x80 + length - 1)
        read(name + "_no_extra_reads", 4, length, inspector=True)
        read(name + "_empty", SR1, 0, 0x44)
        read(name + "_finish", 12, 1, inspector=True)

    # A register-select write must reach the target before repeated-START RX.
    reset()
    address("combined", receive=False)
    clear_address("combined", receive=False)
    script.append(f"sysbus WriteByte 0x{BASE + DR:x} 0x30")
    step()
    read("combined_before_restart_writes", 8, 0, inspector=True)
    write(CR1, PE | START)
    step()
    read("combined_prefix_delivered", 8, 1, inspector=True)
    read("combined_prefix_size", 28, 1, inspector=True)
    read("combined_no_intermediate_finish", 12, 0, inspector=True)
    write(DR, 0xa1)
    step()
    clear_address("combined_read")
    write(CR1, PE | STOP)
    read("combined_selected_register", DR, 0xb0)
    read("combined_read_calls", 4, 1, inspector=True)
    read("combined_finish", 12, 1, inspector=True)

    # A missing target raises AF and keeps it until software writes zero.
    reset(ERR)
    write(CR1, PE | START)
    write(DR, 0xa2)
    step()
    read("nack_irq", 0, 2, inspector=True)
    read("nack_first_read", SR1, 0x400, 0x402)
    read("nack_second_read", SR1, 0x400, 0x402)
    write(SR1, 0x400)
    read("nack_write_one_preserves", SR1, 0x400, 0x400)
    write(SR1, 0)
    step()
    read("nack_write_zero_clears", SR1, 0, 0x400)
    read("nack_irq_cleared", 0, 0, inspector=True)
    write(CR1, PE | STOP)
    read("nack_stop_idle", SR2, 0, 3)
    read("nack_no_target_reads", 4, 0, inspector=True)
    read("nack_no_target_finishes", 12, 0, inspector=True)

    # RXNE requires both buffer and event enables, while BTF requires only EVT.
    reset(EVT)
    address("irq_single")
    read("irq_addr_uses_evt", 0, 1, inspector=True)
    clear_address("irq_single")
    step()
    read("irq_rxne_without_buf", 0, 0, inspector=True)
    write(CR2, 16 | EVT | BUF)
    step()
    read("irq_rxne_with_buf", 0, 1, inspector=True)
    write(CR2, 16 | BUF)
    step()
    read("irq_rxne_without_evt", 0, 0, inspector=True)
    write(CR1, PE | STOP)
    read("irq_single_data", DR, 0x80)
    reset(EVT)
    address("irq_double")
    write(CR1, PE | POS)
    clear_address("irq_double")
    step()
    read("irq_btf_without_buf", 0, 1, inspector=True)
    write(CR2, 16)
    step()
    read("irq_btf_masked", 0, 0, inspector=True)

    # Disable/reset must clear the data path and the actual interrupt outputs.
    write(CR1, 0)
    step()
    read("disable_status1", SR1, 0, 0x4c7)
    read("disable_status2", SR2, 0, 7)
    read("disable_irq", 0, 0, inspector=True)
    reset(EVT | BUF | ERR)
    address("reset_data", ack=True)
    clear_address("reset_data")
    script.append("sysbus.i2c Reset")
    step()
    read("reset_status1", SR1, 0, 0x4c7)
    read("reset_status2", SR2, 0, 7)
    read("reset_irq", 0, 0, inspector=True)
    read("reset_cr1", CR1, 0)
    read("reset_cr2", CR2, 0)
    write(CR1, PE)
    address("after_reset")
    clear_address("after_reset")
    write(CR1, PE | STOP)
    read("after_reset_no_stale_data", DR, 0x82)
    read("after_reset_no_extra_prefetch", 4, 3, inspector=True)

    # A queued completion from a TX write must not resurrect flags after reset.
    reset(EVT | BUF | ERR)
    address("reset_tx", receive=False)
    clear_address("reset_tx", receive=False)
    write(DR, 0xaa)
    script.append("sysbus.i2c Reset")  # Deliberately no scheduler step before reset.
    step()
    read("reset_tx_status1", SR1, 0, 0x4c7)
    read("reset_tx_status2", SR2, 0, 7)
    read("reset_tx_irq", 0, 0, inspector=True)
    read("reset_tx_not_delivered", 8, 0, inspector=True)
    write(CR1, PE)
    address("after_tx_reset", receive=False)
    clear_address("after_tx_reset", receive=False)
    write(DR, 0x30)
    write(CR1, PE | STOP)
    step()
    read("after_tx_reset_one_write", 8, 1, inspector=True)
    read("after_tx_reset_no_stale_prefix", 28, 1, inspector=True)
    read("after_tx_reset_pointer", 24, 0x30, inspector=True)


def test(args, report):
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    for filename in ("report.json", "model.log", "renode.log"):
        (output / filename).unlink(missing_ok=True)
    found = shutil.which(args.renode or os.environ.get("RENODE") or "renode")
    if not found:
        raise ValueError("Renode not found; pass --renode or set RENODE")
    report.update({"model_sha256": hashlib.sha256(MODEL.read_bytes()).hexdigest(),
                   "peer_sha256": hashlib.sha256(PEER.read_bytes()).hexdigest(),
                   "firmware": False, "hal": False,
                   "scope": "direct MMIO with strict counting peer; no electrical timing validation"})
    checks = []
    with tempfile.TemporaryDirectory(prefix="emelino-i2c-model-", dir="/tmp") as temporary:
        alias = Path(temporary) / "run"
        alias.symlink_to(output, target_is_directory=True)
        shutil.copyfile(MODEL, output / MODEL.name)
        shutil.copyfile(PEER, output / PEER.name)
        (output / "machine.repl").write_text(
            'i2c: I2C.EmelinoSTM32F4I2C @ sysbus 0x40005400\n'
            '    EventInterrupt -> inspect@0\n    ErrorInterrupt -> inspect@1\n'
            'peer: I2C.EmelinoCountingPeer @ i2c 0x50\n'
            'inspect: Miscellaneous.EmelinoI2cInspector @ sysbus 0x50000000\n    peer: peer\n')
        script = [f'logFile @{alias}/model.log', 'logLevel 1 file', 'logLevel 3 console',
                  f'include @{alias}/{MODEL.name}', f'include @{alias}/{PEER.name}',
                  'mach create', f'machine LoadPlatformDescription @{alias}/machine.repl']
        scenario(script, checks)
        script.append('quit')
        (output / "run.resc").write_text("\n".join(script) + "\n")
        status, log = run([found, '--disable-xwt', '--console', '--plain', '-e',
                           f'include @{alias}/run.resc'], cwd=ROOT,
                          env=dict(os.environ, XDG_CONFIG_HOME=str(output / 'renode-config'), TMPDIR=temporary),
                          timeout=args.timeout)
        (output / "renode.log").write_text(log)
    model_log = (output / "model.log").read_text(encoding="utf-8-sig")
    version = re.search(r"Renode, version (\S+)", log)
    report['renode_version'] = version[1] if version else 'unknown'
    if status or re.search(r'\[(?:ERROR|WARNING)\]|There was an error executing command|Errors during compilation|Unhandled Exception', log + model_log):
        raise ValueError("Renode error/warning; see renode.log and model.log")
    for check in checks:
        check['actual'] = value(log, check['label'])
        check['passed'] = check['actual'] & check['mask'] == check['expected']
    report['checks'] = checks
    report['check_count'] = len(checks)
    failed = [check for check in checks if not check['passed']]
    if failed:
        raise ValueError("register checks failed: " + repr(failed))
    report['status'] = 'passed'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--renode')
    parser.add_argument('--output-dir', type=Path, default=ROOT / 'out/renode-i2c-model')
    parser.add_argument('--timeout', type=float, default=60)
    args = parser.parse_args()
    report = {"schema_version": 1, "status": "running"}
    try:
        test(args, report)
        print(f"PASS: {report['check_count']} I2C model register/IRQ checks without firmware or HAL")
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        report.update({"status": "failed", "error": str(error)})
        print(error, file=sys.stderr)
    args.output_dir.mkdir(parents=True, exist_ok=True)
    (args.output_dir / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    return 0 if report['status'] == 'passed' else 1


if __name__ == '__main__':
    sys.exit(main())
