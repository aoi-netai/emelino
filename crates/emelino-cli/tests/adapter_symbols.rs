//! C adapter names must remain distinct from the generated ABI and its helpers.
mod support;

use emelino_compiler::{BackendInput, ExternalAdapter, ExternalResult, Options};

const TASK_SOURCE: &str = r#"
effect Wait { suspend fn wait() -> Int }
@external(c, "wait")
suspend fn wait() -> Int
handler Delayed: Wait {
    suspend fn wait(resume) { resume(wait()) }
}
pub fn run() -> Int { with Delayed { (use Wait).wait() } }
"#;

fn input(source: &str) -> BackendInput {
    let generated =
        emelino_cli::frontend::generate(source, std::path::Path::new("main.emel")).unwrap();
    let program =
        emelino_cli::upstream_adapter::adapt(&generated.module, &[("run".into(), "run".into())])
            .unwrap();
    BackendInput::new(program).unwrap()
}

fn rejects(input: &BackendInput, options: &Options, symbol: &str) {
    let error = emelino_cli::errors(input.compile(options).unwrap_err());
    assert!(
        error.contains(symbol) && (error.contains("reserved") || error.contains("conflicts")),
        "{symbol}: {error}"
    );
    let restored = BackendInput::from_json(&input.to_json().unwrap()).unwrap();
    assert_eq!(
        error,
        emelino_cli::errors(restored.compile(options).unwrap_err()),
        "saved input must reject the same adapter"
    );
}

#[test]
fn task_adapters_reject_reserved_names_and_generated_start_frame_helpers() {
    let input = input(TASK_SOURCE);
    for symbol in [
        "emela_scheduler_run",
        "EmelaTask",
        "EMELA_TASK_CAPACITY",
        "while",
        "_Atomic",
        "size_t",
        "int32_t",
        "UINT32_C",
        "NULL",
        "_board_wait",
        "run_start",
        "run_push",
        "run_frame",
        "run_step",
        "run_drop",
    ] {
        rejects(
            &input,
            &Options::default()
                .with_heap_bytes(8192)
                .with_task_adapter("wait", symbol),
            symbol,
        );
    }
}

#[test]
fn synchronous_adapters_share_reserved_name_checks_and_reject_failure_wrapper_names() {
    let input = input(
        r#"
error Missing
@external(c, "value")
fn value() -> Int uses {}
pub fn run(n: Int) -> Int fails Missing {
    if n == 0 { fail Missing } else { value() }
}
"#,
    );
    for symbol in ["EmelaTask", "size_t", "UINT32_C", "run_impl"] {
        rejects(
            &input,
            &Options::default().with_external_adapter(
                "value",
                ExternalAdapter::new(symbol, ExternalResult::Owned),
            ),
            symbol,
        );
    }
}

#[test]
fn task_adapters_cannot_generate_the_same_push_helper_for_distinct_externals() {
    let input = input(
        r#"
effect Wait { suspend fn wait() -> Int }
@external(c, "wait_a")
suspend fn wait_a() -> Int
@external(c, "wait_b")
suspend fn wait_b() -> Int
handler Delayed: Wait {
    suspend fn wait(resume) { resume(wait_a() + wait_b()) }
}
pub fn run() -> Int { with Delayed { (use Wait).wait() } }
"#,
    );
    rejects(
        &input,
        &Options::default()
            .with_heap_bytes(8192)
            .with_task_adapter("wait_a", "board_wait")
            .with_task_adapter("wait_b", "board_wait"),
        "board_wait_push",
    );
}

#[test]
fn task_adapters_reject_the_failure_and_unwind_helpers_when_emitted() {
    let input = input(
        r#"
error Missing
effect Wait { suspend fn wait() -> Int }
@external(c, "wait")
suspend fn wait() -> Int
handler Delayed: Wait {
    suspend fn wait(resume) { resume(wait()) }
    release { dbg(0)
        () }
}
pub fn run() -> Int fails Missing {
    with Delayed {
        n = (use Wait).wait()
        if n == 0 { fail Missing } else { n }
    }
}
"#,
    );
    for symbol in ["run_has_failure", "run_unwind"] {
        rejects(
            &input,
            &Options::default()
                .with_heap_bytes(8192)
                .with_task_adapter("wait", symbol),
            symbol,
        );
    }
}

#[test]
fn task_push_helpers_cannot_collide_with_a_separate_public_function() {
    for source in [
        format!("pub fn other() -> Int {{ 1 }}\n{TASK_SOURCE}"),
        format!("{TASK_SOURCE}\npub fn other() -> Int {{ 1 }}"),
    ] {
        let generated =
            emelino_cli::frontend::generate(&source, std::path::Path::new("main.emel")).unwrap();
        let program = emelino_cli::upstream_adapter::adapt(
            &generated.module,
            &[
                ("run".into(), "run".into()),
                ("other".into(), "board_wait_push".into()),
            ],
        )
        .unwrap();
        rejects(
            &BackendInput::new(program).unwrap(),
            &Options::default()
                .with_heap_bytes(8192)
                .with_task_adapter("wait", "board_wait"),
            "board_wait_push",
        );
    }
}

#[test]
fn valid_task_adapters_execute_after_saved_input_replay_including_unused_suffix_names() {
    // This program has no unwind helper: reserve names actually generated for it,
    // rather than every possible suffix of its public name.
    for adapter in ["board_wait_start", "run_unwind", "run_has_failure"] {
        let output = support::compile(
            TASK_SOURCE,
            &[("run", "run")],
            &Options::default()
                .with_heap_bytes(8192)
                .with_task_adapter("wait", adapter),
        );
        support::compile_and_run(
            &output,
            &format!(
                r#"
#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
static EmelaTask pending;
uint32_t emela_board_event_lock(void) {{ return 0; }}
void emela_board_event_unlock(uint32_t state) {{ (void)state; }}
_Noreturn void emela_board_defect(EmelaDefectKind kind, const char *message, uint16_t length) {{
    (void)kind; (void)message; (void)length; abort();
}}
void {adapter}(EmelaTask task) {{ pending = task; }}
int main(void) {{
    for (int i = 0; i < 32; i++) {{
        int32_t result = -1;
        EmelaFiber fiber = run_start(&result);
        while (emela_scheduler_run(1)) {{}}
        assert(!emela_fiber_done(fiber) && result == -1);
        assert(emela_task_complete_int(pending, 42));
        assert(!emela_task_complete_int(pending, 99));
        while (!emela_fiber_done(fiber)) assert(emela_scheduler_run(1));
        assert(result == 42);
        emela_fiber_release(fiber);
        assert(!emela_task_complete_int(pending, 99));
    }}
    return 0;
}}
"#
            ),
        );
    }
}
