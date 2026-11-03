// Reused from fixed emela-emb b0a367bc0a29227fae62cee2def75ed925a5218f (Apache-2.0).
#[path = "support/heap.rs"]
mod heap;
#[path = "support/legacy.rs"]
mod legacy;
mod support;
use legacy::{Options, Source, compile_sources};

#[test]
fn state_shares_owned_values_with_escaped_closures_and_reclaims_every_run() {
    let output = compile_sources(
        &[Source::new(
            "main.emel",
            include_str!("compat/p4/state.emel"),
        )],
        "main.emel",
        &Options::default()
            .with_heap_bytes(8192)
            .with_string_capacity(256),
    )
    .unwrap_or_else(|errors| panic!("State source rejected: {errors:#?}"));
    let output = heap::with_heap_audit(output);
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
static char input[3];static int trace;
int32_t mark(int32_t value){trace=trace*10+value;return value;}
void mutate_input(void){memset(input,'z',3);}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){fprintf(stderr,"defect %d: %.*s\n",k,n,m);abort();}
int main(void){
  for(int i=0;i<1000;i++){
    assert(state_counter()==83&&test_heap_empty());
    memcpy(input,"a\0b",3);
    assert(state_owned((EmelaString){input,3,NULL},(EmelaString){"a\0b",3,NULL}));
    assert(!memcmp(input,"zzz",3)&&test_heap_empty());
    assert(state_failure((EmelaString){"a\0b",3,NULL})&&test_heap_empty());
    trace=0;assert(state_order()==42&&trace==132&&test_heap_empty());
    assert(state_nested()==42&&test_heap_empty());
    memcpy(input,"a\0b",3);
    assert(state_bytes((EmelaString){input,3,NULL})&&test_heap_empty());
    assert(!memcmp(input,"zzz",3));
    memcpy(input,"a\0b",3);
    EmelaBytes captured=captured_bytes((EmelaString){input,3,NULL});
    memset(input,'x',3);assert(captured.length==3&&captured.data[0]=='a');
    EmelaBytes shared=shared_bytes(captured);emela_embedded_release_bytes(&captured);
    assert(shared.length==6&&shared.data[0]=='a'&&shared.data[4]==0);
    emela_embedded_release_bytes(&shared);assert(test_heap_empty());
  }
  return 0;
}
"#,
    );
}
