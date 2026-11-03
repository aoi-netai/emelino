use super::*;

#[test]
fn unit_any_boxes_round_trip_without_c_storage_and_release_their_heap() {
    let mut f = simple();
    let unit = variable(&mut f, TypeId::UNIT);
    let boxed = aggregate_variable(&mut f, TypeId::ANY);
    let restored = variable(&mut f, TypeId::UNIT);
    let make = binding(&mut f, Some(unit), Value::Const(Const::Unit));
    let box_ = binding(&mut f, Some(boxed), Value::Coerce(unit));
    let unbox = binding(&mut f, Some(restored), Value::Coerce(boxed));
    f.seqs[f.body.unwrap()].stmts = vec![make, box_, unbox];
    let mut output = crate::backend::compile_saved_program(
        program(vec![f]),
        &Options::default().with_heap_bytes(1024),
    )
    .unwrap();
    audit_heap(&mut output);
    execute(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint test_heap_empty(void);\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){for(int i=0;i<1000;i++)if(answer()!=42||!test_heap_empty())return 1;return 0;}\n",
        0,
    );
}
