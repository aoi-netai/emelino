//! Source operations must be normalized before the independent backend boundary.
use super::*;

const SOURCE_ONLY: [Builtin; 8] = [
    Builtin::AssertFail,
    Builtin::Dbg,
    Builtin::DebugPrint,
    Builtin::Inspect,
    Builtin::Panic,
    Builtin::ShowNested,
    Builtin::ShowString,
    Builtin::Todo,
];

#[test]
fn rejects_all_eight_residual_source_builtins_before_emission() {
    for op in SOURCE_ONLY {
        let mut f = simple();
        f.ret = TypeId::UNIT;
        let args = match op {
            Builtin::Todo => vec![],
            Builtin::AssertFail => vec![
                Const::String("assert".into()).into(),
                Const::String("main.emel:1:1".into()).into(),
            ],
            Builtin::Dbg | Builtin::Inspect | Builtin::ShowNested => vec![Const::Int(42).into()],
            _ => vec![Const::String("debug".into()).into()],
        };
        let statement = binding(&mut f, None, Value::Builtin { op, args });
        f.seqs[f.body.unwrap()] = Seq {
            stmts: vec![statement],
            term: Term::Return(None),
        };
        let errors = crate::backend::compile_saved_program(program(vec![f]), &Options::default())
            .unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.kind == crate::ErrorKind::Unsupported),
            "{op:?}: {errors:?}"
        );
    }
}
