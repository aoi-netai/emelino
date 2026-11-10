#!/usr/bin/env node
'use strict';
// A bounded, in-process runner. No daemon, GUI, MCP server or cloud service.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');

function main() {
  const config = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
  if (!['rp2040', 'rp2350-arm'].includes(config.cpu_model)) throw Error('unsupported CPU model');
  if (!(config.max_instructions > 0) || !(config.timeout_ms > 0)) throw Error('invalid execution limit');
  const { RP2040, RP2350 } = require(path.join(config.engine_dir, 'dist/cjs/index.js'));
  const chip = config.cpu_model === 'rp2040' ? new RP2040() : new RP2350({coreArch: 'arm'});
  const warnings = [];
  let warningCount = 0;
  chip.logger = {
    debug() {}, info() {},
    warn(component, message) { if (warnings.length < 30) warnings.push({component, message}); warningCount++; },
    error(component, message) { throw Error(`${component}: ${message}`); },
  };
  chip.loadFirmware(config.firmware);
  const symbols = config.symbols || {};
  const address = (name) => {
    const value = symbols[name];
    if (value === undefined) throw Error(`missing observation symbol: ${name}`);
    return typeof value === 'object' ? value.address : value;
  };
  const read = name => chip.readUint32(address(name));
  const write = (name, value) => chip.writeUint32(address(name), value);
  const has = name => symbols[name] !== undefined;
  const gpio = [];
  for (const pin of [17, 25]) chip.gpio[pin].addListener((value, previous) => {
    if (gpio.length < 100) gpio.push({pin, value, previous});
  });
  chip.gpio[2].setInputValue(true);
  chip.gpio[3].setInputValue(false);
  let uart = '';
  chip.uart[0].onByte = byte => { if (uart.length < 4096) uart += String.fromCharCode(byte); };
  const spiWords = [], spiTransfers = [];
  chip.spi[0].onTransmit = word => {
    if (spiWords.length < 100) spiWords.push(word);
    if (spiTransfers.length < 100) spiTransfers.push({word, bits: chip.spi[0].dataBits,
      cs_output: chip.gpio[17].outputValue, cs_enabled: chip.gpio[17].outputEnable});
    chip.spi[0].completeTransmit(word);
  };
  const memory = new Uint8Array(256);
  memory[208] = 85;
  let pointer = 0, firstByte = true;
  const i2cEvents = [];
  const event = value => { if (i2cEvents.length < 100) i2cEvents.push(value); };
  const bus = chip.i2c[0];
  bus.onStart = repeated => { event({start: true, repeated}); bus.completeStart(); };
  bus.onConnect = (slave, mode) => {
    firstByte = mode === 0;
    event({address: slave, mode});
    bus.completeConnect(slave === 80);
  };
  bus.onWriteByte = byte => {
    if (firstByte) { pointer = byte; firstByte = false; }
    else memory[pointer++ & 255] = byte;
    event({write: byte}); bus.completeWrite(true);
  };
  bus.onReadByte = ack => { const byte = memory[pointer++ & 255]; event({read: byte, ack}); bus.completeRead(byte); };
  bus.onStop = () => { event({stop: true}); bus.completeStop(); };
  const started = Date.now();
  let instructions = 0, reason = 'instruction_limit', error = null, stage = 0, baselineTriggered = false;
  const pcCounts = new Map();
  try {
    for (; instructions < config.max_instructions; instructions++) {
      chip.step();
      if ((instructions & 255) === 0) {
        const pc = chip.core[0].PC >>> 0;
        pcCounts.set(pc, (pcCounts.get(pc) || 0) + 1);
        if (has(config.done_symbol) && read(config.done_symbol)) { reason = 'completed'; break; }
        if (has('emela_board_failures') && read('emela_board_failures')) { reason = 'firmware_defect'; break; }
        if (has('pico_test_stage')) {
          const requested = read('pico_test_stage');
          if (requested > stage) {
            stage = requested;
            if (stage === 1) chip.gpio[3].setInputValue(true);
            if (stage === 2) chip.uart[0].feedByte(65);
            if (stage === 3) chip.uart[0].feedByte(67);
            write('pico_test_continue', stage);
          }
        }
        if (has('pico_baseline_stage') && read('pico_baseline_stage') === 3 && !baselineTriggered) {
          baselineTriggered = true;
          chip.gpio[3].setInputValue(true);
        }
        if (Date.now() - started >= config.timeout_ms) { reason = 'host_timeout'; break; }
      }
    }
  } catch (failure) { reason = 'engine_error'; error = String(failure.stack || failure); }
  const observations = {};
  for (const name of config.observe || []) if (has(name)) observations[name] = read(name);
  const report = {
    cpu_model: config.cpu_model, boot: 'bundled-bootrom-flash-reset', sets_pc_sp: false, sets_vtor: false,
    firmware_sha256: crypto.createHash('sha256').update(fs.readFileSync(config.firmware)).digest('hex'),
    reason, error, instructions, elapsed_ms: Date.now() - started,
    pc: chip.core[0].PC >>> 0, observations, uart, gpio,
    gpio25: {output: chip.gpio[25].outputValue, enabled: chip.gpio[25].outputEnable, function: chip.gpio[25].functionSelect},
    spi_words: spiWords, spi_transfers: spiTransfers, i2c_events: i2cEvents,
    failed_checks: has('pico_test_failure_ids') ? Array.from({length: Math.min(16, read('pico_test_failed'))},
      (_, index) => chip.readUint32(address('pico_test_failure_ids') + 4 * index)) : [],
    hottest_pcs: [...pcCounts].sort((a, b) => b[1] - a[1]).slice(0, 8),
    warnings, warning_count: warningCount,
  };
  fs.writeFileSync(config.report, JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify({reason, instructions, pc: report.pc, observations, uart}));
  return reason === 'completed' ? 0 : 1;
}
try { process.exitCode = main(); }
catch (error) { console.error(error.stack || error); process.exitCode = 2; }
