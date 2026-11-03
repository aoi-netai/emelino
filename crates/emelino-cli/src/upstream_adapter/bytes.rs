//! Explicit external bindings for the emelino-owned Bytes extension.
//!
//! Official ec1031c has no Bytes primitive. The bundled ordinary module declares
//! an opaque external value and a small C ABI; this bridge supplies that ABI
//! through the already tested embedded byte runtime.
use emelino_compiler::ir as d;

const TYPE_NAME: &str = "Emelino.Bytes.Bytes";
const MODULE_PREFIX: &str = "Emelino.Bytes.";
const SYMBOL_PREFIX: &str = "emelino_bytes_";

pub(super) fn identify(source: &emela_lir::Module) -> Result<Option<emela_core::EnumId>, String> {
    let matching: Vec<_> = source
        .core
        .enums
        .iter()
        .filter(|(_, item)| item.name == TYPE_NAME)
        .collect();
    if matching.len() > 1 {
        return Err("duplicate emelino Bytes type declarations".into());
    }
    let Some((id, definition)) = matching.first().copied() else {
        return Ok(None);
    };
    if definition.params != 0
        || !definition.external
        || !definition.variants.is_empty()
        || definition.error
    {
        return Err("emelino Bytes must be an opaque, non-generic external type".into());
    }
    Ok(Some(id))
}

pub(super) fn lower(program: &mut d::Program) -> Result<(), String> {
    let mut updates = Vec::new();
    for (id, function) in program.functions.iter() {
        let Some(external) = &function.external else {
            continue;
        };
        let Some(binding) = external.binding("c") else {
            continue;
        };
        let d::HostRef::Global(symbol) = &binding.host else {
            continue;
        };
        let Some(operation) = symbol.strip_prefix(SYMBOL_PREFIX) else {
            continue;
        };
        let expected_name = format!("{MODULE_PREFIX}{operation}");
        if function.name != expected_name
            || function.fails
            || function.suspends
            || function.constant
            || function.env.is_some()
        {
            return Err(format!("invalid emelino Bytes binding `{}`", function.name));
        }
        let params: Vec<_> = function
            .params
            .iter()
            .map(|&parameter| function.vars[parameter].ty)
            .collect();
        let ret = function.ret;
        let (builtin, valid) = match operation {
            "from_list" => (
                d::Builtin::BytesFromList,
                params.len() == 1
                    && list(program, params[0], d::TypeId::INT)
                    && option(program, ret, d::TypeId::BYTES),
            ),
            "to_list" => (
                d::Builtin::BytesToList,
                params == [d::TypeId::BYTES] && list(program, ret, d::TypeId::INT),
            ),
            "size" => (
                d::Builtin::BytesSize,
                params == [d::TypeId::BYTES] && ret == d::TypeId::INT,
            ),
            "get" => (
                d::Builtin::BytesGet,
                params == [d::TypeId::BYTES, d::TypeId::INT]
                    && option(program, ret, d::TypeId::INT),
            ),
            "slice" => (
                d::Builtin::BytesSlice,
                params == [d::TypeId::BYTES, d::TypeId::INT, d::TypeId::INT]
                    && ret == d::TypeId::BYTES,
            ),
            "concat" => (
                d::Builtin::BytesConcat,
                params.len() == 1
                    && list(program, params[0], d::TypeId::BYTES)
                    && ret == d::TypeId::BYTES,
            ),
            "encode_utf8" => (
                d::Builtin::BytesEncodeUtf8,
                params == [d::TypeId::STRING] && ret == d::TypeId::BYTES,
            ),
            "decode_valid_utf8" => (
                d::Builtin::BytesDecodeValidUtf8,
                params == [d::TypeId::BYTES] && ret == d::TypeId::STRING,
            ),
            _ => return Err(format!("unknown reserved emelino Bytes binding `{symbol}`")),
        };
        if !valid {
            return Err(format!(
                "emelino Bytes binding `{}` has an incompatible signature",
                function.name
            ));
        }
        updates.push((id, builtin));
    }
    for (id, op) in updates {
        let function = &mut program.functions[id];
        let value = function.vars.alloc(d::VarData {
            hint: "bytes_result".into(),
            repr: if function.ret == d::TypeId::INT {
                d::Repr::I32
            } else {
                d::Repr::Ref
            },
            ty: function.ret,
        });
        let statement = function.stmts.alloc(d::Stmt::Let {
            dst: Some(value),
            value: d::Value::Builtin {
                op,
                args: function
                    .params
                    .iter()
                    .copied()
                    .map(d::Operand::Var)
                    .collect(),
            },
            on_defect: None,
            on_suspend: None,
        });
        function.body = Some(function.seqs.alloc(d::Seq {
            stmts: vec![statement],
            term: d::Term::Return(Some(value.into())),
        }));
        function.external = None;
    }
    Ok(())
}

fn list(program: &d::Program, ty: d::TypeId, element: d::TypeId) -> bool {
    matches!(program.core.ty(ty), d::Type::List(actual) if *actual == element)
}

fn option(program: &d::Program, ty: d::TypeId, element: d::TypeId) -> bool {
    let d::Type::Enum(id, args) = program.core.ty(ty) else {
        return false;
    };
    let definition = &program.core.enums[*id];
    definition.name == "Option"
        && !definition.external
        && !definition.error
        && definition.params == 1
        && args == &[element]
        && definition.variants.len() == 2
        && definition.variants[0].name == "Some"
        && definition.variants[0].tys.len() == 1
        && matches!(
            program.core.ty(definition.variants[0].tys[0]),
            d::Type::Param(0)
        )
        && definition.variants[1].name == "None"
        && definition.variants[1].tys.is_empty()
}
