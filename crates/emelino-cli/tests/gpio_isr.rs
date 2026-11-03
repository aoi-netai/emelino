//! Registration sites must prove every possible callback safe, without a user
//! supplied isr_entries list. These use the real source, lowering and C bridge.
use emelino_cli::config::{Config, Export};

const PRELUDE: &str = r#"
import Embedded.Board.{BoardGpio}
import Embedded.Gpio
import Embedded.Interrupt.{Normal}

@external(c, "observe")
fn observe(value: Int) -> () use {}
@external(c, "untrusted")
fn untrusted() -> () use {}

fn safe(event: Gpio.GpioEvent) -> () { observe(1) }
fn bad(event: Gpio.GpioEvent) -> () { untrusted() }
fn install(callback: fn(Gpio.GpioEvent) -> () fails {} use {}) -> () {
  with BoardGpio {
    (use Gpio.Gpio).enable_interrupt(Gpio.GpioPin(0), Gpio.Rising, Normal, callback)
  } escape { Gpio.GpioError(_) -> () }
}
"#;

fn compile(body: &str) -> Result<emelino_compiler::Output, String> {
    emelino_cli::compile(
        &format!("{PRELUDE}\n{body}"),
        &Config {
            exports: vec![Export {
                function: "setup".into(),
                symbol: "setup".into(),
            }],
            heap_bytes: Some(4096),
            isr_safe_externals: vec!["observe".into()],
            ..Config::default()
        },
    )
}

#[test]
fn gpio_isr_resolves_arguments_returns_branches_and_scalar_captures() {
    let output = compile(
        r#"
fn choose(first: fn(Gpio.GpioEvent) -> () fails {} use {},
          unused: fn(Gpio.GpioEvent) -> () fails {} use {})
    -> fn(Gpio.GpioEvent) -> () fails {} use {} { first }
pub fn setup(selector: Bool, tag: Int) -> () {
  callback = if selector { safe } else {
    fn(event: Gpio.GpioEvent) { observe(tag) }
  }
  install(choose(callback, bad))
}
"#,
    )
    .unwrap();
    assert!(output.c_source.contains("emela_gpio_dispatch_interrupt"));
    assert!(!output.c_source.contains("emela_scheduler_run"));
}

#[test]
fn gpio_isr_rejects_an_unsafe_alternative_after_a_function_return() {
    let error = compile(
        r#"
fn choose(selector: Bool) -> fn(Gpio.GpioEvent) -> () fails {} use {} {
  if selector { safe } else { bad }
}
pub fn setup(selector: Bool) -> () { install(choose(selector)) }
"#,
    )
    .unwrap_err();
    assert!(
        error.contains("untrusted") && error.contains("ISR-safe"),
        "{error}"
    );
}

#[test]
fn gpio_isr_does_not_hide_an_unknown_alternative_behind_a_safe_one() {
    let error = compile(
        r#"
pub fn setup(selector: Bool, supplied: fn(Gpio.GpioEvent) -> () fails {} use {}) -> () {
  install(if selector { safe } else { supplied })
}
"#,
    )
    .unwrap_err();
    assert!(
        error.contains("callback targets cannot be determined"),
        "{error}"
    );
}

#[test]
fn gpio_isr_reuses_heap_and_recursion_restrictions() {
    for (body, expected) in [
        (
            r#"
fn allocated(event: Gpio.GpioEvent) -> () {
  [1, 2]
  ()
}
pub fn setup() -> () { install(allocated) }
"#,
            "heap-backed",
        ),
        (
            r#"
fn recurse(value: Int) -> () {
  if value == 0 { () } else {
    recurse(value - 1)
    observe(value)
  }
}

fn recursive(event: Gpio.GpioEvent) -> () { recurse(3) }
pub fn setup() -> () { install(recursive) }
"#,
            "recursive call",
        ),
    ] {
        let error = compile(body).unwrap_err();
        assert!(
            error.contains(expected) && error.contains("GPIO callback"),
            "{error}"
        );
    }
}

#[test]
fn gpio_isr_can_use_checked_board_read_write_toggle_without_allocating_handlers() {
    let output = compile(
        r#"
fn callback(event: Gpio.GpioEvent) -> () {
  with BoardGpio {
    gpio = use Gpio.Gpio
    pin = Gpio.GpioPin(5)
    gpio.write(pin, gpio.read(pin))
    gpio.toggle(pin)
  } escape { Gpio.GpioError(_) -> observe(0) }
}
pub fn setup() -> () { install(callback) }
"#,
    )
    .unwrap();
    let manifest = output
        .files
        .iter()
        .find(|file| file.path == "emela_artifact.json")
        .unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest.contents).unwrap();
    for operation in ["emela_gpio_read", "emela_gpio_write", "emela_gpio_toggle"] {
        assert!(
            manifest["metadata"]["isr_safe_symbols"]
                .as_array()
                .unwrap()
                .iter()
                .any(|symbol| symbol == operation),
            "{manifest}"
        );
    }
}

#[test]
fn gpio_isr_tracks_aggregate_fields_and_indirect_registration_helpers() {
    compile(
        r#"
pub fn setup() -> () {
  pair = (safe, bad)
  pair match {
    (selected, _) -> {
      register = install
      register(selected)
    }
  }
}
"#,
    )
    .unwrap();
}

#[test]
fn gpio_isr_tracks_loop_carried_callback_alternatives() {
    let error = compile(
        r#"
fn select(count: Int, first: fn(Gpio.GpioEvent) -> () fails {} use {},
           second: fn(Gpio.GpioEvent) -> () fails {} use {})
    -> fn(Gpio.GpioEvent) -> () fails {} use {} {
  if count == 0 { first } else { select(count - 1, second, first) }
}
pub fn setup(count: Int) -> () { install(select(count, safe, bad)) }
"#,
    )
    .unwrap_err();
    assert!(
        error.contains("untrusted") && error.contains("ISR-safe"),
        "{error}"
    );
}

#[test]
fn gpio_isr_cannot_configure_or_change_interrupt_registrations() {
    for operation in [
        "gpio.configure(pin, Gpio.Input(Gpio.NoPull))",
        "gpio.enable_interrupt(pin, Gpio.Rising, Normal, safe)",
        "gpio.disable_interrupt(pin)",
    ] {
        let error = compile(&format!(
            r#"
fn callback(event: Gpio.GpioEvent) -> () {{
  with BoardGpio {{
    gpio = use Gpio.Gpio
    pin = Gpio.GpioPin(5)
    {operation}
  }} escape {{ Gpio.GpioError(_) -> () }}
}}
pub fn setup() -> () {{ install(callback) }}
"#
        ))
        .unwrap_err();
        assert!(
            error.contains("not ISR-safe") && error.contains("peripheral operation"),
            "{error}"
        );
    }
}

#[test]
fn gpio_isr_rejects_capture_loads_that_would_retain_managed_values() {
    let error = compile(
        r#"
pub fn setup(text: String) -> () {
  install(fn(event: Gpio.GpioEvent) { observe(String.length(text)) })
}
"#,
    )
    .unwrap_err();
    assert!(
        error.contains("reference-managed closure capture"),
        "{error}"
    );
}

#[test]
fn gpio_isr_unknown_callers_cannot_supply_unchecked_callbacks_to_escaped_installers() {
    let error = compile(r#"
pub fn setup(foreign: fn(fn(fn(Gpio.GpioEvent) -> () fails {} use {}) -> () fails {} use {}) -> () fails {} use {}) -> () {
  install(safe)
  foreign(install)
}
"#).unwrap_err();
    assert!(
        error.contains("callback targets cannot be determined"),
        "{error}"
    );
}

#[test]
fn gpio_isr_foreign_code_cannot_obtain_an_unchecked_installer_through_returned_functions() {
    let error = compile(r#"
fn make_installer() -> fn(fn(Gpio.GpioEvent) -> () fails {} use {}) -> () fails {} use {} {
  install
}
pub fn setup() -> fn() -> fn(fn(Gpio.GpioEvent) -> () fails {} use {}) -> () fails {} use {} fails {} use {} {
  install(safe)
  make_installer
}
"#).unwrap_err();
    assert!(
        error.contains("callback targets cannot be determined"),
        "{error}"
    );
}
