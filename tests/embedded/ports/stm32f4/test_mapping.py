"""Exercise shared scenarios with a temporary board profile and different wiring."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / 'tests/embedded'))
from support.common import load_module, load_target, renode_workspace, run_renode, value
OUT = ROOT / 'out/embedded/stm32f4-mapping'


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    profile = json.loads((ROOT / 'firmware/boards/nucleo-f446re/board.json').read_text())
    target = json.loads((ROOT / 'tests/embedded/targets/nucleo-f446re.json').read_text())
    with tempfile.TemporaryDirectory(prefix='layout-map-', dir=ROOT / 'firmware/boards') as temporary:
        board_dir = Path(temporary)
        board = board_dir.name
        profile['board'] = board
        profile['physical_board'] = 'Temporary configuration regression; not an added hardware target'
        profile['peripherals']['spi1'].update(sck_pin=19, miso_pin=20, mosi_pin=21)
        profile['peripherals']['i2c1'].update(scl_pin=22, sda_pin=23)
        (board_dir / 'board.json').write_text(json.dumps(profile, indent=2))
        (board_dir / 'init.c').write_text('#include <stdint.h>\nvolatile uint32_t layout_init_seen;\nvoid emela_board_init(void) { layout_init_seen = 0x12345678; }\n')
        (OUT / 'board.json').write_text(json.dumps(profile, indent=2))
        target['board'] = board
        target['scenarios'] = {name: target['scenarios'][name] for name in ['bmp180', 'mt25q']}
        target['scenarios']['mt25q']['cs_pin'] = 16
        (OUT / 'target.json').write_text(json.dumps(target, indent=2))
        subprocess.run([sys.executable, 'tools/test_embedded.py', '--board', board, '--target', OUT / 'target.json',
                        '--scenario', 'all', '--output-dir', OUT / 'communication'], cwd=ROOT, check=True)
        loaded = load_target(board, OUT / 'target.json')
        adapter = load_module(ROOT / 'tests/embedded/ports/stm32f4/renode.py', 'mapping_port').Platform(loaded)
        output = OUT / 'registers'
        output.mkdir(exist_ok=True)
        for name in ['model.log', 'renode.log', 'firmware']:
            (output / name).unlink(missing_ok=True)
        (output / 'firmware').symlink_to(OUT / 'communication/mt25q/firmware', target_is_directory=True)
        symbols = json.loads((output / 'firmware/symbols.json').read_text())
        executable = shutil.which(os.environ.get('RENODE', 'renode'))
        if not executable:
            raise ValueError('Renode not found; set RENODE')
        renode = Path(executable).resolve()
        report = {'kind': 'temporary-board-configuration', 'board': board}
        adapter.prepare(renode, output, report)
        settings = target['scenarios']['mt25q']
        with renode_workspace(output) as (alias, temporary_renode):
            (output / 'machine.repl').write_text(adapter.machine_header(alias)
                + 'flashStorage: Memory.MappedMemory @ sysbus 0x90000000\n    size: 0x800000\n'
                + 'serialFlash: SPI.Micron_MT25Q @ spi1\n    underlyingMemory: flashStorage\n'
                + adapter.mt25q_connection(settings))
            script = [f'logFile @{alias}/model.log', 'logLevel 1 file', 'logLevel 3 console', 'mach create',
                      f'machine LoadPlatformDescription @{alias}/machine.repl', adapter.load_binary(alias),
                      'sysbus.cpu PerformanceInMips 100', 'sysbus.cpu Reset', 'emulation RunFor "0:00:00.050"']
            addresses = {'GPIOB_MODER': 0x40020400, 'GPIOB_AFRL': 0x40020420, 'GPIOA_MODER': 0x40020000,
                         'GPIOB_ODR': 0x40020414, 'RCC_AHB1ENR': 0x40023830,
                         'BOARD_INIT': symbols['layout_init_seen'], 'STAGE': symbols['spi_device_test_stage']}
            for name, address in addresses.items():
                script += [f'echo "{name}"', f'sysbus ReadDoubleWord {address:#x}']
            script += ['quit']
            status, log = run_renode(renode, output, alias, temporary_renode, script, report, 60)
        if status or any(term in log for term in ['[ERROR]', 'There was an error', 'Unhandled Exception']):
            raise RuntimeError('register check Renode run failed')
        metrics = {name: value(log, name) for name in addresses}
        assert metrics['BOARD_INIT'] == 0x12345678 and metrics['STAGE'] == 1, metrics
        for pin in [3, 4, 5]:
            assert (metrics['GPIOB_MODER'] >> (pin * 2)) & 3 == 2, metrics
            assert (metrics['GPIOB_AFRL'] >> (pin * 4)) & 15 == 5, metrics
        assert metrics['GPIOB_MODER'] & 3 == 1 and metrics['GPIOB_ODR'] & 1, metrics
        assert metrics['GPIOA_MODER'] & (0x3f << 10) == 0 and metrics['RCC_AHB1ENR'] & 2, metrics
        report.update(status='passed', metrics=metrics, wiring='SPI1 PB3/4/5 AF5, CS PB0; I2C1 PB6/7 AF4',
                      board_init_called=True, scenarios=['bmp180', 'mt25q'])
        (OUT / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print('PASS: alternate board wiring, shared BMP180/MT25Q scenarios, pin registers and board init hook')


if __name__ == "__main__":
    main()
