//! Legacy compiler-generated services with the fixed native C provider as a test fixture.
#[path = "support/heap.rs"]
mod heap;
#[path = "support/legacy.rs"]
mod legacy;
mod support;
use emelino_compiler::Options;
use legacy::{Source, compile_sources};
fn run(source: &str, name: &str, harness: &str) {
    let output = heap::with_heap_audit(support::compile(
        source,
        &[(name, &format!("legacy_{name}"))],
        &Options::default()
            .with_heap_bytes(4096)
            .with_frame_bytes(2048),
    ));
    let console = if output.files.iter().any(|f| f.path == "emela_console.h") {
        "#define EMELA_REQUIRE_SERVICE_CONSOLE 1\n"
    } else {
        ""
    };
    let native = include_str!("compat/legacy-external-sources/native-services.c");
    support::compile_and_run(
        &output,
        &format!(
            "#define EMELA_REQUIRE_SERVICE_CLOCK 1\n#define EMELA_PLATFORM_TASK_CAPACITY 8\n{console}{native}\n{harness}"
        ),
    );
}
#[test]
fn original_native_clock_console_and_timer_use_generated_common_services() {
    run(
        include_str!(
            "compat/legacy-external-sources/5549983f8aba12d57b0d789684cba2e5255b99a5874cbfa1674dbb16533483e2.emel"
        ),
        "main",
        r#"
#include <assert.h>
int test_heap_empty(void);
int main(void) {
    int32_t result=-1;EmelaFiber f=legacy_main_start(&result);
    while (!emela_fiber_done(f)) {emela_scheduler_run(64);emela_board_idle();}
    assert(result==42);emela_fiber_release(f);assert(test_heap_empty());
    return 0;
}
"#,
    );
}
#[test]
fn original_native_timer_slots_survive_cancellation_and_repeated_reuse() {
    run(
        include_str!(
            "compat/legacy-external-sources/d60b4137b5256577f8ab5618caf2ea81c9b88bdcd375988cc79f6ee071a7de12.emel"
        ),
        "wait_ms",
        r#"
#include <assert.h>
int test_heap_empty(void);
int main(void) {
  for (int round=0;round<1000;++round) {
    int32_t result=-1;EmelaFiber fiber=legacy_wait_ms_start(&result);
    while (emela_scheduler_run(64)) {}
    assert(!emela_scheduler_ready());
    emela_board_idle();
    assert(emela_scheduler_ready() && result==-1);
    for (int i=0;i<1000 && !emela_fiber_done(fiber);++i) {emela_scheduler_run(64);emela_board_idle();}
    assert(emela_fiber_done(fiber) && result==42);emela_fiber_release(fiber);
    assert(test_heap_empty());
  }
  return 0;
}
"#,
    );
}

#[test]
fn board_clock_sleep_clamps_boundaries_and_notifies_before_main_resumes() {
    let source = r#"
import Std.Clock.{Clock}
import Embedded.Board.{BoardClock}
@export(c,"board_wait")
pub fn wait_then_return(ms:Int)->Int use Async {
  with BoardClock {(use Clock).sleep(ms)
    42}
}
"#;
    let output = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default()
            .with_heap_bytes(8192)
            .with_string_capacity(96)
            .with_peripheral_buffer_capacity(4)
            .with_fiber_capacity(4)
            .with_frame_capacity(12)
            .with_frame_bytes(2048)
            .with_task_capacity(2)
            .with_event_capacity(2)
            .with_deferred_capacity(1),
    )
    .unwrap_or_else(|errors| panic!("BoardClock source rejected: {errors:#?}"));
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_clock.h"
#include "emela_board.h"
#ifndef EMELA_HAS_TASK_RUNTIME
#error "Clock.sleep requires the task runtime"
#endif
static EmelaTask pending_task;
static int pending;
static int32_t requested;
static unsigned starts;
static uint32_t locked;
int64_t emela_board_clock_now(void){return 0;}
uint32_t emela_board_event_lock(void){uint32_t old=locked;locked=1;return old;}
void emela_board_event_unlock(uint32_t old){locked=old;}
void emela_board_clock_sleep(EmelaTask task,int32_t ms){assert(!locked&&!pending);pending_task=task;requested=ms;pending=1;starts++;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){const int32_t delays[]={INT32_MIN,-1,0,7,INT32_MAX};for(unsigned i=0;i<1000;i++){
 int32_t ms=delays[i%5],result=-1;EmelaFiber fiber=board_wait_start(ms,&result);
 while(emela_scheduler_run(1)){}
 assert(pending&&starts==i+1&&requested==(ms<0?0:ms));
 assert(result==-1&&!emela_fiber_done(fiber));pending=0;
 assert(emela_task_complete_unit(pending_task)&&!emela_task_complete_unit(pending_task));
 assert(result==-1&&!emela_fiber_done(fiber));
 while(!emela_fiber_done(fiber))assert(emela_scheduler_run(1));
 assert(result==42&&!locked);emela_fiber_release(fiber);
 assert(!emela_task_complete_unit(pending_task));
}return 0;}
"#,
    );
}
