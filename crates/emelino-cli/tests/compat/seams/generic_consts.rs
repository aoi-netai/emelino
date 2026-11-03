//! Fixed generic const observations; UPSTREAM-001 differences are recorded explicitly.
use super::*;

const FIXTURES: &[(&str, &str)] = &[
    (
        "090d8d",
        include_str!(
            "../legacy-corpus/sources/8390bd800c6070a0b68164ddec005d4af36073bf8487c68ee1da26b9383082f5.emel"
        ),
    ),
    (
        "0e55f9",
        include_str!(
            "../legacy-corpus/sources/5f7efbeaef62d31c1e1f46eaad6e92bcd39f6d285327b6245e4c93ba70c54b4c.emel"
        ),
    ),
    (
        "1e18a1",
        include_str!(
            "../legacy-corpus/sources/267ecea81443467d0abeecdbe3a9226b9453747c353cc442957d562d46dcb1a5.emel"
        ),
    ),
    (
        "4ec3db",
        include_str!(
            "../legacy-corpus/sources/823e637e97a0dbeb7ea04e02088af922b3433413c10b0a79c4211b2a7046493d.emel"
        ),
    ),
    (
        "6509a2",
        include_str!(
            "../legacy-corpus/sources/c986b4c8e5754de0a2f9b2fe0109553e9bd049a3528d9a8aec33ba2b0bce6bfb.emel"
        ),
    ),
    (
        "82d224",
        include_str!(
            "../legacy-corpus/sources/fa1ee5cc165e68a73803369872071c65b263140058becc944b02fd1a8c54e56b.emel"
        ),
    ),
    (
        "9d483c",
        include_str!(
            "../legacy-corpus/sources/a958e5ba357850a0c4de5cbfeca3159b8894275fc1bba1b25fafd2a0b8d9c187.emel"
        ),
    ),
    (
        "cd203f",
        include_str!(
            "../legacy-corpus/sources/c9f0f411c6dcdc3595139fa3e214523be9e023c3b2f44b16b16a07f32cdf0953.emel"
        ),
    ),
];

// UPSTREAM-001: the official Flow retains distinct generic const identities.
// Old shared-family counts (2/1/1/1/2 ticks+6 logs) are recorded in the report;
// these assert the observed per-specialization counts without changing values,
// retry/cache behavior, or ownership assertions.
fn run(id: &str) {
    let original = FIXTURES.iter().find(|(name, _)| *name == id).unwrap().1;
    let program = adapted_public(original, "");
    let mut output =
        emelino_compiler::compile_program(program, &Options::default().with_heap_bytes(32768))
            .unwrap_or_else(|errors| panic!("legacy {id}: {errors:#?}"));
    let heap_check = if output.c_source.contains("emela_heap_storage") {
        "int test_heap_empty(void){for(unsigned char*p=emela_heap_storage;emela_heap_initialized&&p<emela_heap_end();){emela_heap_block*b=(emela_heap_block*)(void*)p;if(!b->is_free)return 0;p+=b->size;}return 1;}\n"
    } else {
        "int test_heap_empty(void){return 1;}\n"
    };
    output
        .files
        .iter_mut()
        .find(|file| file.path == "emela_embedded.c")
        .unwrap()
        .contents
        .push_str(heap_check);
    let headers = "#include <stdio.h>\n#include <stdlib.h>\n#include <setjmp.h>\n#include <string.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint test_heap_empty(void);\n";
    let abort = "_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\n";
    let debug = "static int logs;void emela_board_debug(const char*s,uint16_t n){(void)s;(void)n;logs++;}\n";
    let (hooks, main) = match id {
            "090d8d" => (abort.to_owned(), "int main(void){for(int i=0;i<1000;i++)if(work()!=42||!test_heap_empty())return 1;return 0;}\n".to_owned()),
            "0e55f9" => (format!("{abort}{debug}"), "int main(void){for(int i=0;i<1000;i++)if(text()!=42||integer()!=42||logs!=3)return 1;return 0;}\n".to_owned()),
            "1e18a1" => (abort.to_owned(), "int main(void){for(int i=0;i<1000;i++){EmelaString s=text();if(integer()!=42||s.length!=2||memcmp(s.data,\"42\",2))return 1;}return 0;}\n".to_owned()),
            "4ec3db" | "6509a2" => {
                let recursive = id == "6509a2";
                let expected = if recursive {"const BROKEN[%s] refers to itself while being evaluated"} else {"while evaluating const BROKEN[%s]: broken"};
                let hooks = format!("{debug}\nstatic jmp_buf jump;static int calls,defects;int32_t trigger(void){{calls++;return 1;}}\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){{(void)k;char expected[128];int size=snprintf(expected,sizeof(expected),\"{expected}\",defects%2==0?\"Int\":\"String\");if(n!=size||memcmp(m,expected,n))abort();defects++;longjmp(jump,1);}}\n");
                let count = if recursive {2000} else {2};
                let main = format!("int main(void){{for(volatile int i=0;i<1000;i++){{if(setjmp(jump)==0){{integer();abort();}}if(setjmp(jump)==0){{text();abort();}}if(!test_heap_empty())return 1;}}return calls!={count}||logs!={count}||defects!=2000;}}\n");
                (hooks, main)
            },
            "82d224" => (format!("{abort}{debug}"), "int main(void){for(int i=0;i<1000;i++)if(integer()!=42||text()!=42||logs!=2)return 1;return 0;}\n".to_owned()),
            "9d483c" => (format!("{abort}{debug}"), "int main(void){for(int i=0;i<1000;i++)if(read()!=0||logs!=2||!test_heap_empty())return 1;return 0;}\n".to_owned()),
            "cd203f" => (format!("{abort}{debug}\nstatic int ticks,marks;int32_t tick(void){{return ++ticks;}}void mark(int32_t n){{if(n!=2)abort();marks++;}}\n"), "int main(void){for(int i=0;i<1000;i++)if(integer()!=42||text()!=42||ticks!=4||logs!=12||marks!=2*(i+1))return 1;return 0;}\n".to_owned()),
            _ => unreachable!(),
        };
    execute(&output, &format!("{headers}{hooks}{main}"), 0);
}

macro_rules! original_case {
    ($name:ident,$id:literal) => {
        #[test]
        fn $name() {
            run($id);
        }
    };
}
original_case!(empty_list_keeps_heapless_representation, "090d8d");
original_case!(nested_specializations_cache_each_received_const, "0e55f9");
original_case!(
    generic_function_references_keep_type_specific_calls,
    "1e18a1"
);
original_case!(generic_defects_cache_each_received_const, "4ec3db");
original_case!(
    generic_self_reference_retries_without_cached_failure,
    "6509a2"
);
original_case!(
    nested_list_keeps_values_across_const_specializations,
    "82d224"
);
original_case!(empty_lists_follow_received_const_identities, "9d483c");
original_case!(generic_closure_families_keep_values_and_branches, "cd203f");
