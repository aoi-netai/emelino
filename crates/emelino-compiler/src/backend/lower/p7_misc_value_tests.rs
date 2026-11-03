use super::*;

#[test]
fn int64_decimal_conversion_uses_exactly_twenty_bytes_of_scratch() {
    let mut f = simple();
    f.ret = TypeId::BOOL;
    let input = variable(&mut f, TypeId::INT64);
    let expected = aggregate_variable(&mut f, TypeId::STRING);
    f.params = vec![input, expected];
    let text = aggregate_variable(&mut f, TypeId::STRING);
    let convert = binding(
        &mut f,
        Some(text),
        Value::Builtin {
            op: Builtin::Int64ToString,
            args: vec![input.into()],
        },
    );
    let same = variable(&mut f, TypeId::BOOL);
    let compare = binding(
        &mut f,
        Some(same),
        Value::Prim {
            op: PrimOp::Eq,
            ty: PrimTy::String,
            args: vec![text.into(), expected.into()],
        },
    );
    f.seqs[f.body.unwrap()] = Seq {
        stmts: vec![convert, compare],
        term: Term::Return(Some(same.into())),
    };
    let output =
        crate::backend::compile_saved_program(program(vec![f]), &Options::default()).unwrap();
    assert!(output.c_source.contains("char emela_string_buffer_0[20];"));
    execute(
        &output,
        "#include <string.h>\n#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\n#define TEXT(s) ((EmelaString){s,sizeof(s)-1,NULL})\nint main(void){for(int i=0;i<1000;i++)if(!answer(INT64_MIN,TEXT(\"-9223372036854775808\"))||!answer(INT64_MAX,TEXT(\"9223372036854775807\"))||!answer(0,TEXT(\"0\")))return 1;return 0;}\n",
        0,
    );
}

#[test]
fn checked_add_preserves_all_four_legacy_option_boundaries() {
    let mut p = program(vec![]);
    let parameter = p.core.types.intern(Type::Param(0));
    let option = p.core.enums.alloc(EnumDef {
        name: "Option".into(),
        params: 1,
        error: false,
        external: false,
        variants: vec![
            VariantDef {
                name: "Some".into(),
                fields: VariantFields::Positional(1),
                tys: vec![parameter],
            },
            VariantDef {
                name: "None".into(),
                fields: VariantFields::Unit,
                tys: vec![],
            },
        ],
    });
    let ty = p.core.types.enum_type(option, vec![TypeId::INT]);
    let c_type = crate::semantics::c::type_name(ty);
    let mut f = simple();
    f.ret = ty;
    let left = variable(&mut f, TypeId::INT);
    let right = variable(&mut f, TypeId::INT);
    f.params = vec![left, right];
    let result = aggregate_variable(&mut f, ty);
    let add = binding(
        &mut f,
        Some(result),
        Value::Builtin {
            op: Builtin::IntCheckedAdd,
            args: vec![left.into(), right.into()],
        },
    );
    f.seqs[f.body.unwrap()] = Seq {
        stmts: vec![add],
        term: Term::Return(Some(result.into())),
    };
    p.functions.alloc(f);
    let output = crate::backend::compile_saved_program(p, &Options::default()).unwrap();
    let harness = format!(
        "#include \"emela_embedded.h\"\nint main(void){{for(int i=0;i<1000;i++){{{c_type} x=answer(INT32_MAX,1);if(x.tag!=1)return 1;x=answer(INT32_MIN,-1);if(x.tag!=1)return 2;x=answer(INT32_MAX,0);if(x.tag!=0||x.payload.v0.f0!=INT32_MAX)return 3;x=answer(-2,1);if(x.tag!=0||x.payload.v0.f0!=-1)return 4;}}return 0;}}\n"
    );
    execute(&output, &harness, 0);
}

#[test]
fn code_point_lists_require_a_heap_even_when_only_the_head_is_returned() {
    let mut p = program(vec![]);
    let list = p.core.types.intern(Type::List(TypeId::INT));
    let mut f = simple();
    let input = aggregate_variable(&mut f, TypeId::STRING);
    f.params.push(input);
    let points = aggregate_variable(&mut f, list);
    let result = variable(&mut f, TypeId::INT);
    let build = binding(
        &mut f,
        Some(points),
        Value::Builtin {
            op: Builtin::StringCodePoints,
            args: vec![input.into()],
        },
    );
    let head = binding(&mut f, Some(result), Value::ListHead(points));
    f.seqs[f.body.unwrap()] = Seq {
        stmts: vec![build, head],
        term: Term::Return(Some(result.into())),
    };
    p.functions.alloc(f);
    let errors = crate::backend::compile_saved_program(p, &Options::default()).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.kind == ErrorKind::Unsupported && e.message.contains("heap_bytes = 0")),
        "{errors:?}"
    );
}

#[test]
fn split_join_keeps_the_old_multibyte_value_and_capacity_five_defect() {
    let mut p = program(vec![]);
    let list = p.core.types.intern(Type::List(TypeId::STRING));
    let mut f = simple();
    f.ret = TypeId::BOOL;
    let input = aggregate_variable(&mut f, TypeId::STRING);
    f.params.push(input);
    let parts = aggregate_variable(&mut f, list);
    let joined = aggregate_variable(&mut f, TypeId::STRING);
    let same = variable(&mut f, TypeId::BOOL);
    let split = binding(
        &mut f,
        Some(parts),
        Value::Builtin {
            op: Builtin::StringSplit,
            args: vec![input.into(), Const::String("::".into()).into()],
        },
    );
    let join = binding(
        &mut f,
        Some(joined),
        Value::Builtin {
            op: Builtin::StringJoin,
            args: vec![parts.into(), Const::String("|".into()).into()],
        },
    );
    let compare = binding(
        &mut f,
        Some(same),
        Value::Prim {
            op: PrimOp::Eq,
            ty: PrimTy::String,
            args: vec![joined.into(), Const::String("alpha|βeta".into()).into()],
        },
    );
    f.seqs[f.body.unwrap()] = Seq {
        stmts: vec![split, join, compare],
        term: Term::Return(Some(same.into())),
    };
    p.functions.alloc(f);
    let options = Options::default().with_heap_bytes(2048);
    let mut output = crate::backend::compile_saved_program(p.clone(), &options).unwrap();
    audit_owned_heap(&mut output);
    execute(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint test_heap_empty(void);\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){for(int i=0;i<1000;i++)if(!answer((EmelaString){\"alpha::βeta\",12,NULL})||!test_heap_empty())return 1;return 0;}\n",
        0,
    );
    let output =
        crate::backend::compile_saved_program(p, &options.with_string_capacity(5)).unwrap();
    execute(
        &output,
        "#include <stdlib.h>\n#include <string.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){exit(k==EMELA_DEFECT_STRING_CAPACITY&&n==24&&!memcmp(m,\"string capacity exceeded\",24)?0:2);}\nint main(void){(void)answer((EmelaString){\"alpha::beta\",11,NULL});return 1;}\n",
        0,
    );
}

#[test]
fn debug_print_passes_the_original_string_view_to_the_board_hook() {
    let mut f = simple();
    f.ret = TypeId::UNIT;
    let text = aggregate_variable(&mut f, TypeId::STRING);
    f.params.push(text);
    let print = binding(
        &mut f,
        None,
        Value::Runtime {
            op: RuntimeOp::DebugPrint,
            args: vec![text.into()],
        },
    );
    f.seqs[f.body.unwrap()] = Seq {
        stmts: vec![print],
        term: Term::Return(None),
    };
    let output =
        crate::backend::compile_saved_program(program(vec![f]), &Options::default()).unwrap();
    let board = &output
        .files
        .iter()
        .find(|file| file.path == "emela_board.h")
        .unwrap()
        .contents;
    assert!(board.contains("void emela_board_debug(const char *bytes, uint16_t length);"));
    assert!(!board.contains("emela_board_defect"));
    execute(
        &output,
        "#include <stdlib.h>\n#include <string.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nstatic int calls;\nvoid emela_board_debug(const char *p,uint16_t n){if(n!=3||memcmp(p,\"abc\",3))exit(1);calls++;}\nint main(void){for(int i=0;i<1000;i++)answer((EmelaString){\"xabc\"+1,3,NULL});return calls==1000?0:2;}\n",
        0,
    );
}

#[test]
fn float_primitive_cases_keep_the_fixed_old_arithmetic_comparison_and_nan_results() {
    let cases = [
        (
            PrimOp::Add,
            vec![Const::Float(1.25), Const::Float(2.75)],
            TypeId::FLOAT,
            "actual == 4.0",
        ),
        (
            PrimOp::Sub,
            vec![Const::Float(4.5), Const::Float(1.25)],
            TypeId::FLOAT,
            "actual == 3.25",
        ),
        (
            PrimOp::Mul,
            vec![Const::Float(1.5), Const::Float(2.0)],
            TypeId::FLOAT,
            "actual == 3.0",
        ),
        (
            PrimOp::Div,
            vec![Const::Float(7.0), Const::Float(2.0)],
            TypeId::FLOAT,
            "actual == 3.5",
        ),
        (
            PrimOp::Rem,
            vec![Const::Float(7.0), Const::Float(2.0)],
            TypeId::FLOAT,
            "actual == 1.0",
        ),
        (
            PrimOp::Neg,
            vec![Const::Float(-2.5)],
            TypeId::FLOAT,
            "actual == 2.5",
        ),
        (
            PrimOp::Eq,
            vec![Const::Float(1.25), Const::Float(1.25)],
            TypeId::BOOL,
            "actual == INT32_C(1)",
        ),
        (
            PrimOp::Ne,
            vec![Const::Float(1.25), Const::Float(2.0)],
            TypeId::BOOL,
            "actual == INT32_C(1)",
        ),
        (
            PrimOp::Lt,
            vec![Const::Float(-1.0), Const::Float(0.0)],
            TypeId::BOOL,
            "actual == INT32_C(1)",
        ),
        (
            PrimOp::Le,
            vec![Const::Float(2.0), Const::Float(2.0)],
            TypeId::BOOL,
            "actual == INT32_C(1)",
        ),
        (
            PrimOp::Gt,
            vec![Const::Float(3.0), Const::Float(2.0)],
            TypeId::BOOL,
            "actual == INT32_C(1)",
        ),
        (
            PrimOp::Ge,
            vec![Const::Float(2.0), Const::Float(2.0)],
            TypeId::BOOL,
            "actual == INT32_C(1)",
        ),
        (
            PrimOp::Eq,
            vec![Const::Float(f64::NAN), Const::Float(0.0)],
            TypeId::BOOL,
            "actual == INT32_C(0)",
        ),
        (
            PrimOp::Ne,
            vec![Const::Float(f64::NAN), Const::Float(0.0)],
            TypeId::BOOL,
            "actual == INT32_C(1)",
        ),
    ];
    for (op, args, result_type, expected) in cases {
        let mut f = simple();
        f.ret = result_type;
        let result = variable(&mut f, result_type);
        let operation = binding(
            &mut f,
            Some(result),
            Value::Prim {
                op,
                ty: PrimTy::F64,
                args: args.into_iter().map(Into::into).collect(),
            },
        );
        f.seqs[f.body.unwrap()] = Seq {
            stmts: vec![operation],
            term: Term::Return(Some(result.into())),
        };
        let output =
            crate::backend::compile_saved_program(program(vec![f]), &Options::default()).unwrap();
        let ty = if result_type == TypeId::FLOAT {
            "double"
        } else {
            "int32_t"
        };
        let harness = format!(
            "#include <math.h>\n#include <stdint.h>\n#include \"emela_embedded.h\"\nint main(void){{{ty} actual=answer();return ({expected})?0:1;}}\n"
        );
        execute(&output, &harness, 0);
    }
}
