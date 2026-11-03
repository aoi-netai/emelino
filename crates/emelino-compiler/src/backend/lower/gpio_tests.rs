use super::*;

fn typed_variable(function: &mut Function, ty: TypeId) -> VarId {
    let variable = super::variable(function, ty);
    if function.vars[variable].repr == Repr::Void && ty != TypeId::UNIT {
        function.vars[variable].repr = Repr::Ref;
    }
    variable
}

#[test]
fn gpio_factory_does_not_specialize_a_loop_carried_handler_on_the_false_branch() {
    let mut p = program(vec![]);
    let callback_type = p.core.types.intern(Type::Fn(FnType {
        params: vec![],
        ret: TypeId::UNIT,
        fails: false,
        suspends: false,
        ev: false,
        sets: None,
    }));
    let operation_params = vec![TypeId::INT, TypeId::INT, TypeId::INT, callback_type];
    let operation_type = p.core.types.intern(Type::Fn(FnType {
        params: operation_params.clone(),
        ret: TypeId::INT,
        fails: false,
        suspends: false,
        ev: false,
        sets: None,
    }));
    let effect = p.core.effects.alloc(EffectDef {
        name: "Embedded.Gpio.Gpio".into(),
        decl: "Embedded.Gpio.Gpio".into(),
        args: vec![],
        label_of: None,
        ops: vec![OpDef {
            name: "enable_interrupt".into(),
            suspend: false,
            type_params: 0,
            params: operation_params.clone(),
            ret: TypeId::INT,
            fails: false,
        }],
    });
    let handler_type = p.core.types.intern(Type::Handler(effect));
    let mut targets = Vec::new();
    for (name, result) in [("board_operation", 42), ("other_operation", 7)] {
        let mut f = Function::new(name);
        f.ret = TypeId::INT;
        for &ty in &operation_params {
            let parameter = typed_variable(&mut f, ty);
            f.params.push(parameter);
        }
        f.body = Some(f.seqs.alloc(Seq {
            stmts: vec![],
            term: Term::Return(Some(Const::Int(result).into())),
        }));
        targets.push(p.functions.alloc(f));
    }
    let mut factory = Function::new("Embedded.Board.BoardGpio");
    factory.ret = handler_type;
    let operation = typed_variable(&mut factory, operation_type);
    let handler = typed_variable(&mut factory, handler_type);
    let reference = binding(&mut factory, Some(operation), Value::FnRef(targets[0]));
    let construct = binding(
        &mut factory,
        Some(handler),
        Value::Handler {
            name: "BoardGpio".into(),
            effect,
            ops: vec![operation],
        },
    );
    factory.body = Some(factory.seqs.alloc(Seq {
        stmts: vec![reference, construct],
        term: Term::Return(Some(handler.into())),
    }));
    let factory = p.functions.alloc(factory);
    let mut noop = Function::new("noop");
    noop.ret = TypeId::UNIT;
    noop.body = Some(noop.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Return(None),
    }));
    let noop = p.functions.alloc(noop);

    let mut entry = simple();
    let flag = typed_variable(&mut entry, TypeId::BOOL);
    entry.params.push(flag);
    let other_op = typed_variable(&mut entry, operation_type);
    let other = typed_variable(&mut entry, handler_type);
    let h = typed_variable(&mut entry, handler_type);
    let callback = typed_variable(&mut entry, callback_type);
    let result = typed_variable(&mut entry, TypeId::INT);
    let reference = binding(&mut entry, Some(other_op), Value::FnRef(targets[1]));
    let construct = binding(
        &mut entry,
        Some(other),
        Value::Handler {
            name: "Other".into(),
            effect,
            ops: vec![other_op],
        },
    );
    let callback_reference = binding(&mut entry, Some(callback), Value::FnRef(noop));
    let replacement = binding(
        &mut entry,
        Some(h),
        Value::Call {
            callee: Callee::Direct(factory),
            args: vec![],
            suspends: false,
            on_fail: None,
        },
    );
    let yes = entry.seqs.alloc(Seq {
        stmts: vec![replacement],
        term: Term::Yield(None),
    });
    let no = entry.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Yield(None),
    });
    let condition = entry.stmts.alloc(Stmt::If {
        dst: None,
        cond: flag.into(),
        then: yes,
        else_: no,
    });
    let invoke = binding(
        &mut entry,
        Some(result),
        Value::Call {
            callee: Callee::Op {
                handler: h,
                effect,
                op: 0,
            },
            args: vec![
                Const::Int(0).into(),
                Const::Int(0).into(),
                Const::Int(0).into(),
                callback.into(),
            ],
            suspends: false,
            on_fail: None,
        },
    );
    let body = entry.seqs.alloc(Seq {
        stmts: vec![condition, invoke],
        term: Term::Return(Some(result.into())),
    });
    let repeat = entry.stmts.alloc(Stmt::Loop {
        dst: None,
        vars: vec![h],
        inits: vec![other.into()],
        body,
    });
    entry.seqs[entry.body.unwrap()] = Seq {
        stmts: vec![reference, construct, callback_reference, repeat],
        term: Term::Unreachable,
    };
    p.functions.alloc(entry);
    let output =
        crate::backend::compile_saved_program(p, &Options::default().with_heap_bytes(4096))
            .unwrap();
    execute(
        &output,
        r#"
#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) {
  (void)k; (void)m; (void)n; abort();
}
int main(void) {
  for (int i = 0; i < 100; ++i) { assert(answer(0) == 7); assert(answer(1) == 42); }
  return 0;
}
"#,
        0,
    );
}
