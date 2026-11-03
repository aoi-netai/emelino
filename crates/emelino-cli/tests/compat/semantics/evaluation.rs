//! Reused source/C observations from emela-emb b0a367bc (Apache-2.0).
use super::support;

use super::legacy::{Options, Source, compile_sources};
use support::compile_and_run;

fn source_output(source: &str) -> emelino_compiler::Output {
    compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default(),
    )
    .unwrap_or_else(|errors| panic!("P1 source rejected: {errors:#?}"))
}

#[test]
fn short_circuiting_and_argument_evaluation_preserve_side_effect_order() {
    let output = source_output(
        r#"
@external(c, "board_bool")
fn tick() -> Bool use {}
@external(c, "board_value")
fn value(n: Int) -> Int use {}
fn combine(a: Int, b: Int) -> Int { a * 10 + b }
@export(c, "check_order")
pub fn check_order() -> Int {
  assert !(False && tick())
  assert True || tick()
  assert True && tick()
  assert !(False || !tick())
  combine(value(1), value(2))
}
"#,
    );
    compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdbool.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { (void)k; (void)m; (void)n; abort(); }
static int ticks;
static int ordered;
int32_t board_bool(void) { ticks++; return 1; }
int32_t board_value(int32_t n) { ordered = ordered * 10 + n; return n; }
int main(void) { assert(check_order() == 12); assert(ticks == 2); assert(ordered == 12); return 0; }
"#,
    );
}

#[test]
fn unused_division_remainder_and_negative_shifts_still_raise_defects() {
    for (expression, kind, expected) in [
        ("a / b", "EMELA_DEFECT_DIVISION_BY_ZERO", "division by zero"),
        ("a % b", "EMELA_DEFECT_DIVISION_BY_ZERO", "division by zero"),
        (
            "Int.shift_left(a, b)",
            "EMELA_DEFECT_NEGATIVE_SHIFT",
            "negative shift amount: -2147483648",
        ),
        (
            "Int.shift_right(a, b)",
            "EMELA_DEFECT_NEGATIVE_SHIFT",
            "negative shift amount: -2147483648",
        ),
        (
            "Int.shift_right_unsigned(a, b)",
            "EMELA_DEFECT_NEGATIVE_SHIFT",
            "negative shift amount: -2147483648",
        ),
    ] {
        for ty in ["Int", "Int64"] {
            let expression = if ty == "Int64" {
                expression.replace("Int.", "Int64.")
            } else {
                expression.into()
            };
            let argument_type = if expression.contains("shift_") {
                "Int"
            } else {
                ty
            };
            let source = format!(
                "@export(c, \"probe\")\npub fn probe(a: {ty}, b: {argument_type}) -> () {{ unused = {expression}\n () }}"
            );
            let output = source_output(&source);
            let divisor = if expression.contains("shift_") {
                "INT32_MIN"
            } else {
                "0"
            };
            let harness = format!(
                r#"
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) {{
  static const char expected[] = "{expected}";
  exit(k == {kind} && n == sizeof expected - 1 && memcmp(m, expected, n) == 0 ? 0 : 1);
}}
int main(void) {{ probe(7, {divisor}); return 2; }}
"#
            );
            compile_and_run(&output, &harness);
        }
    }
}
