//! v0.27 Array behavior through the public frontend, persisted input, and strict C11.
#[path = "support/heap.rs"]
mod heap;
mod support;
use emelino_compiler::Options;

#[test]
fn arrays_preserve_values_ownership_and_standard_helpers() {
    let source = r#"
import Std.Array.{Array}
type Grid(cells: Array[String]) derive Eq, Show
enum Tree { Leaf(String), Branch(Array[Tree]) } derive Eq
fn saved() -> Array[String] { Array.from_list(["v#{42}", "tail"]) }
fn key(p: (Int, String)) -> Int { p match { (k, _) -> k } }
pub fn answer() -> Bool {
  tree = Branch(Array.from_list([Leaf("tree#{1}")]))
  grid = Grid(Array.from_list(["cell"]))
  a = Array.from_list([3, 1, 2, 1])
  b = Array.set(a, 0, 9)
  s = Array.sort(a)
  pairs = Array.from_list([(2, "a"), (1, "b"), (2, "c"), (1, "d")])
  texts = saved()
  one = Array.get(texts, 0) |> Option.or("")
  edited = Array.set(texts, 1, "new#{7}")
  nested = Array.from_list([Array.from_list([1]), Array.from_list([2, 3])])
  unit = Array.from_list([(), (), ()])
  fs = Array.from_list([|x: Int| x + 1, |x: Int| x * 2])
  tree == Branch(Array.from_list([Leaf("tree1")]))
  && Array.get(Array.from_list([True, False]), 1) == Some(False)
  && Array.get(Array.from_list([Int64.from_int(-2)]), 0) == Some(Int64.from_int(-2))
  && (Array.get(Array.from_list([1.5, -0.0]), 0) |> Option.or(0.0)) == 1.5
  && grid == Grid(Array.from_list(["cell"]))
  && inspect(grid) == "Grid(cells: Array.from_list([\"cell\"]))"
  && a == Array.from_list([3, 1, 2, 1]) && b == Array.from_list([9, 1, 2, 1])
  && Array.to_list(s) == [1, 1, 2, 3]
  && Array.get(a, -1) == None && Array.get(a, 4) == None
  && Array.slice(a, -4, 2) == Array.from_list([3, 1])
  && Array.size(Array.slice(a, 3, 2)) == 0
  && Array.concat([Array.empty(), a, s]) == Array.append(a, s)
  && Array.reverse(s) == Array.from_list([3, 2, 1, 1])
  && Array.map(s, |x| x * 2) == Array.from_list([2, 2, 4, 6])
  && Array.fold(s, 0, |acc, x| acc + x) == 7
  && Array.find_index(a, |x| x == 2) == Some(2)
  && Array.sort_by(pairs, |x, y| key(x).compare(key(y)))
       == Array.from_list([(1, "b"), (1, "d"), (2, "a"), (2, "c")])
  && Array.lower_bound(s, 1) == 0 && Array.binary_search(s, 2) == Some(2)
  && Array.binary_search(s, 4) == None
  && one == "v42" && Array.to_list(edited) == ["v42", "new7"]
  && Array.to_list(texts) == ["v42", "tail"]
  && Array.to_list(Array.get(nested, 1) |> Option.or(Array.empty())) == [2, 3]
  && Array.size(unit) == 3 && Array.get(unit, 1) == Some(())
  && Array.fold(fs, 5, |acc, f| f(acc)) == 12
  && inspect(Array.from_list(["a"])) == "Array.from_list([\"a\"])"
  && Array.from_list([1, 2]) < Array.from_list([1, 2, 0])
}
"#;
    let output = heap::with_heap_audit(support::compile(
        source,
        &[("answer", "answer")],
        &Options::default()
            .with_heap_bytes(65536)
            .with_string_capacity(256),
    ));
    support::compile_and_run(
        &output,
        r#"
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){for(int i=0;i<100;i++){if(!answer())return 1;if(!test_heap_empty())return 2;}return 0;}
"#,
    );
}

#[test]
fn unique_last_uses_update_in_place_through_tail_recursion() {
    let source = r#"
import Std.Array.{Array}
fn fill(a: Array[Int], i: Int) -> Array[Int] {
  if i == 1000 { a } else { fill(Array.set(a, 0, i), i + 1) }
}
pub fn answer() -> Int {
  a = Array.from_list([0, 1, 2, 3])
  b = fill(a, 0)
  Array.get(b, 0) |> Option.or(-1)
}
"#;
    let mut output = heap::with_heap_audit(support::compile(
        source,
        &[("answer", "answer")],
        &Options::default().with_heap_bytes(2048),
    ));
    let c = &mut output
        .files
        .iter_mut()
        .find(|f| f.path == "emela_embedded.c")
        .unwrap()
        .contents;
    let mut instrumented = String::from("static unsigned test_array_copies;\n");
    for line in c.lines() {
        instrumented.push_str(line);
        instrumented.push('\n');
        if line.starts_with("static inline EMELA_UNUSED")
            && line.contains("_copy(const emela_array_")
        {
            instrumented.push_str("  ++test_array_copies;\n");
        }
    }
    instrumented.push_str("unsigned test_copies(void){return test_array_copies;}\n");
    *c = instrumented;
    support::compile_and_run(
        &output,
        r#"
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void); unsigned test_copies(void);
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
#include <stdio.h>
int main(void){for(int i=0;i<100;i++){int v=answer();if(v!=999){fprintf(stderr,"answer=%d copies=%u\n",v,test_copies());return 1;}if(!test_heap_empty())return 2;}return test_copies()!=0;}
"#,
    );
}

#[test]
fn borrowed_calls_and_live_captures_keep_the_original_array() {
    let source = r#"
import Std.Array.{Array}
fn bump(a: Array[Int]) -> Array[Int] { Array.set(a, 0, 99) }
fn repeated(a: Array[Int], b: Array[Int]) -> Bool {
  changed = Array.set(a, 0, 88)
  Array.get(b, 0) == Some(1) && Array.get(changed, 0) == Some(88)
}
fn cycle(a: Array[Int], i: Int) -> Array[Int] {
  if i == 0 { cycle(Array.from_list([7, 8]), 1) } else {
    reader = || Array.get(a, 0)
    b = bump(a)
    if reader() == Some(7) { b } else { Array.empty() }
  }
}
pub fn answer() -> Bool {
  once = Array.from_list([1, 2])
  duplicate_ok = repeated(once, once)
  a = Array.from_list([1, 2])
  b = bump(a)
  c = if Array.size(a) > 0 { bump(a) } else { a }
  reader = || Array.get(a, 0)
  t = (a, "kept")
  d = Array.set(a, 0, 42)
  duplicate_ok && Array.get(a, 0) == Some(1) && Array.get(b, 0) == Some(99)
  && Array.get(c, 0) == Some(99) && reader() == Some(1)
  && (t match { (original, _) -> Array.get(original, 0) == Some(1) })
  && Array.get(d, 0) == Some(42) && Array.get(cycle(a, 0), 0) == Some(99)
}
"#;
    let output = heap::with_heap_audit(support::compile(
        source,
        &[("answer", "answer")],
        &Options::default().with_heap_bytes(16384),
    ));
    support::compile_and_run(
        &output,
        r#"
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){for(int i=0;i<100;i++){if(!answer())return 1;if(!test_heap_empty())return 2;}return 0;}
"#,
    );
}

#[test]
fn arrays_captured_after_updates_keep_their_original_values() {
    for update in ["Array.set(a, 0, 99)", "bump(a)"] {
        let source = format!(
            r#"
import Std.Array.{{Array}}
fn bump(a: Array[Int]) -> Array[Int] {{ Array.set(a, 0, 99) }}
pub fn answer() -> Bool {{
  a = Array.from_list([1, 2])
  b = {update}
  reader = || Array.get(a, 0)
  reader() == Some(1) && Array.get(b, 0) == Some(99)
}}
"#
        );
        let output = heap::with_heap_audit(support::compile(
            &source,
            &[("answer", "answer")],
            &Options::default().with_heap_bytes(4096),
        ));
        support::compile_and_run(
            &output,
            r#"
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){for(int i=0;i<100;i++){if(!answer())return 1;if(!test_heap_empty())return 2;}return 0;}
"#,
        );
    }
}

#[test]
fn arrays_captured_after_calls_remain_alive() {
    let source = r#"
import Std.Array
fn id(x: Int) -> Int { x }
pub fn answer() -> Bool {
  a = Array.from_list([1, 2])
  n = id(1)
  reader = || Array.get(a, 0)
  n == 1 && reader() == Some(1)
}
"#;
    let output = heap::with_heap_audit(support::compile(
        source,
        &[("answer", "answer")],
        &Options::default().with_heap_bytes(4096),
    ));
    support::compile_and_run(
        &output,
        r#"
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){for(int i=0;i<100;i++){if(!answer())return 1;if(!test_heap_empty())return 2;}return 0;}
"#,
    );
}

#[test]
fn array_bounds_defects_survive_discarding_and_exit_attempt() {
    for index in [-1, i32::MIN, 1] {
        let literal = if index == i32::MIN {
            "(-2147483647 - 1)".into()
        } else {
            index.to_string()
        };
        let source = format!(
            r#"
import Std.Array
import Std.Exit
pub fn answer() -> Int {{
  Exit.attempt(|| {{ Array.set(Array.from_list([1]), {literal}, 2)
    0 }})
  7
}}
"#
        );
        let output = support::compile(
            &source,
            &[("answer", "answer")],
            &Options::default().with_heap_bytes(4096),
        );
        support::compile_and_run(
            &output,
            &format!(
                r#"
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){{const char*expected="Array.set was given position {index} in an array of size 1";exit(k==EMELA_DEFECT_PANIC&&strlen(expected)==n&&!memcmp(m,expected,n)?0:1);}}
int main(void){{answer();return 2;}}
"#
            ),
        );
    }
}

#[test]
fn arrays_cross_deferred_results_suspension_and_cancellation_without_leaks() {
    let source = r#"
import Std.Array
import Std.Fiber
import Std.Deferred
pub fn answer() -> Bool {
  Fiber.scope(|| {
    d = Fiber.deferred()
    child = Fiber.fork(|| {
      a = Deferred.await(d)
      Array.set(a, 0, "child#{42}")
    })
    a = Array.from_list(["parent#{7}", "tail"])
    Deferred.complete(d, a)
    b = Fiber.join(child)
    pending = Fiber.deferred()
    blocked = Fiber.fork(|| {
      owned = Array.from_list(["blocked#{1}", "payload"])
      n: Int = Deferred.await(pending)
      Array.set(owned, 0, "blocked#{n}")
    })
    turn = Fiber.fork(|| 1)
    Fiber.join(turn)
    Fiber.interrupt(blocked)
    Array.get(a, 0) == Some("parent7") && Array.get(b, 0) == Some("child42")
  })
}
"#;
    let output = heap::with_heap_audit(support::compile(
        source,
        &[("answer", "answer")],
        &Options::default()
            .with_heap_bytes(32768)
            .with_frame_bytes(2048)
            .with_frame_capacity(32)
            .with_fiber_capacity(8)
            .with_deferred_capacity(4),
    ));
    support::compile_and_run(
        &output,
        r#"
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){for(int i=0;i<100;i++){int32_t result=0;EmelaFiber f=answer_start(&result);while(!emela_fiber_done(f))if(!emela_scheduler_run(1))return 1;emela_fiber_release(f);if(!result)return 2;if(!test_heap_empty())return 3;}return 0;}
"#,
    );
}

#[test]
fn public_c_array_arguments_remain_borrowed() {
    let source = r#"
import Std.Array.{Array}
pub fn make() -> Array[Int] { Array.from_list([1, 2]) }
pub fn change(a: Array[Int]) -> Array[Int] { Array.set(a, 0, 9) }
pub fn read(a: Array[Int]) -> Int { Array.get(a, 0) |> Option.or(-1) }
"#;
    let output = heap::with_heap_audit(support::compile(
        source,
        &[("make", "make"), ("change", "change"), ("read", "read")],
        &Options::default().with_heap_bytes(4096),
    ));
    support::compile_and_run(
        &output,
        r#"
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){for(int i=0;i<100;i++){void*a=make();void*b=change(a);if(a==b||read(a)!=1||read(b)!=9)return 1;emela_embedded_release(a);emela_embedded_release(b);if(!test_heap_empty())return 2;}return 0;}
"#,
    );
}

#[test]
fn public_c_array_arguments_remain_borrowed_through_internal_calls() {
    let source = r#"
import Std.Array.{Array}
fn bump(a: Array[Int]) -> Array[Int] { Array.set(a, 0, 99) }
fn apply(a: Array[Int], f: fn(Array[Int]) -> Array[Int]) -> Array[Int] { f(a) }
pub fn make() -> Array[Int] { Array.from_list([1, 2]) }
pub fn forwarded(a: Array[Int]) -> Array[Int] { bump(a) }
pub fn callback(a: Array[Int]) -> Array[Int] { apply(a, bump) }
pub fn looped(a: Array[Int], n: Int) -> Array[Int] {
  if n == 0 { bump(a) } else { looped(a, n - 1) }
}
pub fn read(a: Array[Int]) -> Int { Array.get(a, 0) |> Option.or(-1) }
"#;
    let output = heap::with_heap_audit(support::compile(
        source,
        &[
            ("make", "make"),
            ("forwarded", "forwarded"),
            ("callback", "callback"),
            ("looped", "looped"),
            ("read", "read"),
        ],
        &Options::default().with_heap_bytes(4096),
    ));
    support::compile_and_run(
        &output,
        r#"
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){
  for(int i=0;i<100;i++){
    void*a=make();
    void*b=forwarded(a);void*c=callback(a);void*d=looped(a,3);
    if(a==b||a==c||a==d||read(a)!=1||read(b)!=99||read(c)!=99||read(d)!=99)return 1;
    emela_embedded_release(a);emela_embedded_release(b);emela_embedded_release(c);emela_embedded_release(d);
    if(!test_heap_empty())return 2;
  }
  return 0;
}
"#,
    );
}

#[test]
fn constant_arrays_keep_their_cache_and_copy_before_updates() {
    let source = r#"
import Std.Array.{Array}
const VALUES = Array.from_list(["cached#{7}", "tail"])
pub fn cached() -> Array[String] { VALUES }
pub fn changed() -> Array[String] { Array.set(VALUES, 0, "new#{8}") }
pub fn first(a: Array[String]) -> Bool { Array.get(a, 0) == Some("cached7") }
"#;
    let output = heap::with_heap_audit(support::compile(
        source,
        &[
            ("cached", "cached"),
            ("changed", "changed"),
            ("first", "first"),
        ],
        &Options::default().with_heap_bytes(8192),
    ));
    support::compile_and_run(
        &output,
        r#"
#include <stdlib.h>
#include <stddef.h>
#include "emela_embedded.h"
#include "emela_board.h"
size_t test_heap_used(void);
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){void*warm=cached();emela_embedded_release(warm);size_t baseline=test_heap_used();for(int i=0;i<100;i++){void*a=cached();void*b=cached();void*c=changed();if(a!=b||a==c||!first(a)||first(c))return 1;emela_embedded_release(a);emela_embedded_release(b);emela_embedded_release(c);if(test_heap_used()!=baseline)return 2;}return 0;}
"#,
    );
}

#[test]
fn saved_array_assignments_do_not_inherit_an_earlier_bindings_last_use() {
    use emelino_compiler::ir::{Builtin, Stmt, Value};
    let source = r#"
import Std.Array
pub fn answer() -> Bool {
  a = Array.from_list([1])
  one = Array.set(a, 0, 2)
  before = Array.size(one)
  b = Array.from_list([3])
  two = Array.set(b, 0, 4)
  before == 1 && Array.get(two, 0) == Some(4) && Array.get(b, 0) == Some(3)
}
"#;
    let flow = emelino_cli::frontend::generate(source, std::path::Path::new("main.emel")).unwrap();
    let mut program =
        emelino_cli::upstream_adapter::adapt(&flow.module, &[("answer".into(), "answer".into())])
            .unwrap();
    let f = program
        .functions
        .iter_mut()
        .find(|(_, f)| f.name == "answer")
        .unwrap()
        .1;
    let sets: Vec<_> = f
        .stmts
        .iter()
        .filter_map(|(id, s)| match s {
            Stmt::Let {
                dst: Some(dst),
                value:
                    Value::Builtin {
                        op: Builtin::ArraySet,
                        ..
                    },
                ..
            } => Some((id, *dst)),
            _ => None,
        })
        .collect();
    assert_eq!(sets.len(), 2);
    let old = sets[1].1;
    let reused = sets[0].1;
    if let Stmt::Let { dst, .. } = &mut f.stmts[sets[1].0] {
        *dst = Some(reused);
    }
    for (_, s) in f.stmts.iter_mut() {
        if let Stmt::Let { value, .. } = s {
            value.for_each_var_mut(|v| {
                if *v == old {
                    *v = reused;
                }
            });
        }
    }
    let saved = emelino_compiler::BackendInput::new(program)
        .unwrap()
        .to_json()
        .unwrap();
    let output = heap::with_heap_audit(
        emelino_compiler::BackendInput::from_json(&saved)
            .unwrap()
            .compile(&Options::default().with_heap_bytes(4096))
            .unwrap(),
    );
    support::compile_and_run(
        &output,
        r#"
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){if(!answer())return 1;return !test_heap_empty();}
"#,
    );
}
