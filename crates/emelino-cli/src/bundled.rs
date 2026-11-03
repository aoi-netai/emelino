//! Emelino-owned extension modules. These use ordinary public source APIs.
//! Embedded modules are reused from emela-emb b0a367bc0a29227fae62cee2def75ed925a5218f.

pub(crate) const SOURCES: &[(&str, &str)] = &[
    (
        "emelino/bytes.emel",
        include_str!("../lib/emelino/bytes.emel"),
    ),
    (
        "embedded/peripheral.emel",
        include_str!("../lib/embedded/peripheral.emel"),
    ),
    (
        "embedded/gpio.emel",
        include_str!("../lib/embedded/gpio.emel"),
    ),
    (
        "embedded/uart.emel",
        include_str!("../lib/embedded/uart.emel"),
    ),
    (
        "embedded/i2c.emel",
        include_str!("../lib/embedded/i2c.emel"),
    ),
    (
        "embedded/spi.emel",
        include_str!("../lib/embedded/spi.emel"),
    ),
    (
        "embedded/board.emel",
        include_str!("../lib/embedded/board.emel"),
    ),
];
