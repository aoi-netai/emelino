//! Translate upstream's erased Unit callback result to the typed embedded Any ABI.
//!
//! In pinned upstream from_core::box_result, a callback without a represented
//! return value is wrapped as fn(...) -> Any and returns an empty String as an
//! arbitrary reference. The embedded ABI uses a tagged Unit box instead. Match
//! the complete typed wrapper shape, never an empty String by itself.
use emelino_compiler::ir as d;
use std::collections::BTreeSet;

pub(super) fn normalize(program: &mut d::Program) {
    for (_, function) in program.functions.iter_mut() {
        let mut bodies = BTreeSet::new();
        for (_, statement) in function.stmts.iter() {
            let d::Stmt::Let {
                dst: Some(dst),
                value: d::Value::Closure(closure),
                ..
            } = statement
            else {
                continue;
            };
            let d::Type::Fn(closure_type) = program.core.ty(function.vars[*dst].ty) else {
                continue;
            };
            if closure_type.ret != d::TypeId::ANY {
                continue;
            }
            let body = &function.seqs[closure.body];
            if !matches!(&body.term, d::Term::Return(Some(d::Operand::Const(d::Const::String(value)))) if value.is_empty())
                || body.stmts.len() != 1
            {
                continue;
            }
            let d::Stmt::Let {
                dst: None,
                value:
                    d::Value::Call {
                        callee: d::Callee::Indirect(callee),
                        args,
                        suspends,
                        on_fail,
                    },
                on_defect: None,
                on_suspend: None,
            } = &function.stmts[body.stmts[0]]
            else {
                continue;
            };
            let d::Type::Fn(callee_type) = program.core.ty(function.vars[*callee].ty) else {
                continue;
            };
            if callee_type.ret == d::TypeId::UNIT
                && callee_type.params == closure_type.params
                && callee_type.ev == closure_type.ev
                && callee_type.fails == closure_type.fails
                && callee_type.suspends == closure_type.suspends
                && callee_type.suspends == *suspends
                && callee_type.fails == on_fail.is_some()
                && closure.captures.contains(callee)
                && args
                    == &closure
                        .params
                        .iter()
                        .copied()
                        .map(d::Operand::Var)
                        .collect::<Vec<_>>()
            {
                bodies.insert(closure.body);
            }
        }
        for body in bodies {
            let unit = function.vars.alloc(d::VarData {
                hint: "unit_result".into(),
                repr: d::Repr::Void,
                ty: d::TypeId::UNIT,
            });
            let boxed = function.vars.alloc(d::VarData {
                hint: "boxed_unit".into(),
                repr: d::Repr::Ref,
                ty: d::TypeId::ANY,
            });
            let make = function.stmts.alloc(d::Stmt::Let {
                dst: Some(unit),
                value: d::Value::Const(d::Const::Unit),
                on_defect: None,
                on_suspend: None,
            });
            let boxing = function.stmts.alloc(d::Stmt::Let {
                dst: Some(boxed),
                value: d::Value::Coerce(unit),
                on_defect: None,
                on_suspend: None,
            });
            function.seqs[body].stmts.extend([make, boxing]);
            function.seqs[body].term = d::Term::Return(Some(boxed.into()));
        }
    }
}
