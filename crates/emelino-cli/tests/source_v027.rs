//! New v0.27 source contracts, including tests of the frozen-source migration seam.
mod support;
use emelino_compiler::Options;

#[test]
fn wildcard_escape_handles_errors_and_keeps_arm_order_and_effects() {
    let output = support::compile(
        r#"
import Std.Exit
import Std.Cause
error First(n: Int)
error Second
fn failer(n: Int) -> Int fails First | Second {
  if n == 0 { fail First(7) } else { fail Second }
}
fn fallback[E](f: fn() -> Int fails E) -> Int { f() escape { _ -> 42 } }
pub fn answer() -> Bool {
  a = failer(0) escape { First(n) -> n
    _ -> 99 }
  b = failer(1) escape { First(_) -> 0
    _ -> 8 }
  a == 7 && b == 8 && fallback(|| failer(1)) == 42
  && (Exit.attempt(|| fail Second) match {
    Exit.Success(_) -> False
    Exit.Failure(c) -> Cause.kind(c) == Cause.Fail
  })
}
"#,
        &[("answer", "answer")],
        &Options::default().with_heap_bytes(8192),
    );
    support::compile_and_run(
        &output,
        r#"
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){return answer()!=1;}
"#,
    );
}

#[test]
fn frozen_source_migration_preserves_strings_comments_and_function_types() {
    let old = r#"
import Std.Fiber.{Fiber, Exit, Success, Failure}
# Fiber.attempt and fn(x) { x } use Db
fn pure(f: fn(Int) -> Int use {}) -> Int use {} { f(2) }
pub fn answer() -> String {
  result = Fiber.attempt(fn() { pure(fn(x) { x + 1 }) })
  result match { Success(_) -> "Fiber.attempt(fn() {}) use Db"
    Failure(_) -> "wrong" }
}
"#;
    let migrated = support::migrate::source(old);
    assert!(migrated.contains("# Fiber.attempt and fn(x) { x } use Db"));
    assert!(migrated.contains("\"Fiber.attempt(fn() {}) use Db\""));
    assert!(migrated.contains("fn(Int) -> Int uses {}"));
    assert!(emela_syntax::parse(&migrated).diagnostics().is_empty());
    assert_eq!(support::migrate::source(&migrated), migrated);
}

#[test]
fn product_frontend_rejects_obsolete_syntax() {
    for (source, code) in [
        ("pub fn f() -> Int use {} { 1 }", "E0147"),
        ("pub fn f() -> Int { (fn(x) { x + 1 })(1) }", "E0148"),
    ] {
        let error = emelino_cli::frontend::generate(source, std::path::Path::new("main.emel"))
            .err()
            .unwrap();
        assert!(error.contains(code), "{error}");
    }
}
