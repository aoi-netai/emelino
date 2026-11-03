//! Exhaustive projection from verified upstream Flow to the owned backend contract.
//!
//! Adapted from emela-codegen-emb/src/compiler/lower/adapter.rs at
//! c34c0aeefdd966d854371b3110db00a47798751d (Apache-2.0).
//! No upstream lowering pass or Core function body is examined here.
use emelino_compiler::ir as d;
use std::collections::{HashMap, HashSet};

mod bytes;
mod cause;
mod unit_any;
use la_arena::{Arena, Idx};

fn id<T, U>(value: Idx<T>) -> Idx<U> {
    Idx::from_raw(value.into_raw())
}
// IDs belong to their respective producers. The mapping also connects the
// explicit emelino Bytes extension to the reused backend's reserved Bytes type.
struct TypeMap {
    ids: Vec<d::TypeId>,
    bytes: Option<emela_core::EnumId>,
}

impl TypeMap {
    fn new(source: &emela_core::Module, bytes: Option<emela_core::EnumId>) -> Self {
        let mut next = 11;
        let ids = source.types.iter().enumerate().map(|(index, node)| {
            if index < 10 {
                d::TypeId::from_index(index)
            } else if matches!(node, emela_core::Type::Enum(id, args) if Some(*id) == bytes && args.is_empty()) {
                d::TypeId::BYTES
            } else {
                let result = d::TypeId::from_index(next);
                next += 1;
                result
            }
        }).collect();
        Self { ids, bytes }
    }

    fn get(&self, value: emela_core::TypeId) -> d::TypeId {
        self.ids[value.index()]
    }
}

fn arena<T, U>(input: &Arena<T>, convert: impl Fn(&T) -> U) -> Arena<U> {
    let mut result = Arena::new();
    for (_, value) in input.iter() {
        result.alloc(convert(value));
    }
    result
}

/// Convert raw Flow while selecting the explicitly configured public C roots.
pub fn adapt(
    source: &emela_lir::Module,
    exports: &[(String, String)],
) -> Result<d::Program, String> {
    if source.stage != emela_lir::Stage::Flow {
        return Err("emelino Flow adapter requires raw Stage::Flow".into());
    }
    emela_lir::verify(source)
        .map_err(|errors| format!("emelino Flow adapter received invalid Flow: {errors:#?}"))?;
    let mut selected = HashMap::new();
    let mut symbols = HashSet::new();
    for (name, symbol) in exports {
        if selected.insert(name.as_str(), symbol.as_str()).is_some() {
            return Err(format!("duplicate configured function `{name}`"));
        }
        if !symbols.insert(symbol) {
            return Err(format!("duplicate configured C name `{symbol}`"));
        }
        let mut matching = source.functions.iter().filter(|(_, f)| f.name == *name);
        let Some((_, function)) = matching.next() else {
            return Err(format!(
                "configured function `{name}` is absent from upstream Flow"
            ));
        };
        if matching.next().is_some() {
            return Err(format!("configured function name `{name}` is ambiguous"));
        }
        if !function.exported {
            return Err(format!(
                "configured function `{name}` is not an upstream public root"
            ));
        }
        if function.params.iter().any(|parameter| {
            matches!(
                source.core.types[function.vars[*parameter].ty],
                emela_core::Type::Evidence
            )
        }) {
            return Err(format!(
                "configured function `{name}` has unresolved effect evidence; supply a concrete public wrapper with its handlers"
            ));
        }
    }
    let type_map = TypeMap::new(&source.core, bytes::identify(source)?);
    let mut core = definitions(&source.core, &type_map)?;
    let mut functions = Arena::new();
    for (_, function) in source.functions.iter() {
        let mut converted = adapt_function(function, &mut core, &type_map)?;
        converted.export_symbol = selected
            .get(function.name.as_str())
            .map(|name| (*name).to_owned());
        converted.exported = converted.export_symbol.is_some();
        functions.alloc(converted);
    }
    let mut program = d::Program {
        core,
        functions,
        error_shows: source
            .error_shows
            .iter()
            .map(|(e, f)| (id(*e), id(*f)))
            .collect(),
        // Upstream currently publishes each const specialization separately. No
        // origin reconstruction or private identity convention is introduced.
        const_function_groups: Vec::new(),
        has_scopes: false,
    };
    bytes::lower(&mut program)?;
    unit_any::normalize(&mut program);
    cause::normalize(&mut program)?;
    Ok(program)
}

fn definitions(source: &emela_core::Module, type_map: &TypeMap) -> Result<d::Definitions, String> {
    let mut types = d::TypeTable::default();
    for (index, ty) in source.types.iter().enumerate() {
        let actual = types.intern(adapt_type(ty, type_map));
        if actual != type_map.ids[index] {
            return Err("upstream type table does not preserve canonical identity".into());
        }
    }
    Ok(d::Definitions {
        types,
        enums: arena(&source.enums, |item| adapt_enum_def(item, type_map)),
        effects: arena(&source.effects, |item| adapt_effect_def(item, type_map)),
        // The backend derives recursive layout information after validation.
        expanding_enums: HashSet::new(),
    })
}

fn adapt_builtin(source: &emela_core::Builtin) -> d::Builtin {
    match source {
        emela_core::Builtin::StringLength => d::Builtin::StringLength,
        emela_core::Builtin::StringByteSize => d::Builtin::StringByteSize,
        emela_core::Builtin::StringAppend => d::Builtin::StringAppend,
        emela_core::Builtin::StringContains => d::Builtin::StringContains,
        emela_core::Builtin::StringStartsWith => d::Builtin::StringStartsWith,
        emela_core::Builtin::StringEndsWith => d::Builtin::StringEndsWith,
        emela_core::Builtin::StringSplit => d::Builtin::StringSplit,
        emela_core::Builtin::StringChars => d::Builtin::StringChars,
        emela_core::Builtin::StringJoin => d::Builtin::StringJoin,
        emela_core::Builtin::StringTrim => d::Builtin::StringTrim,
        emela_core::Builtin::StringCodePoints => d::Builtin::StringCodePoints,
        emela_core::Builtin::StringFromCodePoint => d::Builtin::StringFromCodePoint,
        emela_core::Builtin::ShowNested => d::Builtin::ShowNested,
        emela_core::Builtin::IntToString => d::Builtin::IntToString,
        emela_core::Builtin::IntParse => d::Builtin::IntParse,
        emela_core::Builtin::IntCheckedAdd => d::Builtin::IntCheckedAdd,
        emela_core::Builtin::IntBitAnd => d::Builtin::IntBitAnd,
        emela_core::Builtin::IntBitOr => d::Builtin::IntBitOr,
        emela_core::Builtin::IntBitXor => d::Builtin::IntBitXor,
        emela_core::Builtin::IntBitNot => d::Builtin::IntBitNot,
        emela_core::Builtin::IntShiftLeft => d::Builtin::IntShiftLeft,
        emela_core::Builtin::IntShiftRight => d::Builtin::IntShiftRight,
        emela_core::Builtin::IntShiftRightUnsigned => d::Builtin::IntShiftRightUnsigned,
        emela_core::Builtin::Int64FromInt => d::Builtin::Int64FromInt,
        emela_core::Builtin::Int64ToString => d::Builtin::Int64ToString,
        emela_core::Builtin::Int64ToInt => d::Builtin::Int64ToInt,
        emela_core::Builtin::Int64BitAnd => d::Builtin::Int64BitAnd,
        emela_core::Builtin::Int64BitOr => d::Builtin::Int64BitOr,
        emela_core::Builtin::Int64BitXor => d::Builtin::Int64BitXor,
        emela_core::Builtin::Int64BitNot => d::Builtin::Int64BitNot,
        emela_core::Builtin::Int64ShiftLeft => d::Builtin::Int64ShiftLeft,
        emela_core::Builtin::Int64ShiftRight => d::Builtin::Int64ShiftRight,
        emela_core::Builtin::Int64ShiftRightUnsigned => d::Builtin::Int64ShiftRightUnsigned,
        emela_core::Builtin::FloatToString => d::Builtin::FloatToString,
        emela_core::Builtin::FloatFromInt => d::Builtin::FloatFromInt,
        emela_core::Builtin::FloatParse => d::Builtin::FloatParse,
        emela_core::Builtin::FloatFloor => d::Builtin::FloatFloor,
        emela_core::Builtin::FloatCeil => d::Builtin::FloatCeil,
        emela_core::Builtin::FloatRound => d::Builtin::FloatRound,
        emela_core::Builtin::FloatTruncate => d::Builtin::FloatTruncate,
        emela_core::Builtin::Panic => d::Builtin::Panic,
        emela_core::Builtin::Todo => d::Builtin::Todo,
        emela_core::Builtin::Dbg => d::Builtin::Dbg,
        emela_core::Builtin::Inspect => d::Builtin::Inspect,
        emela_core::Builtin::CellNew => d::Builtin::CellNew,
        emela_core::Builtin::CellGet => d::Builtin::CellGet,
        emela_core::Builtin::CellSet => d::Builtin::CellSet,
        emela_core::Builtin::ShowString => d::Builtin::ShowString,
        emela_core::Builtin::DebugPrint => d::Builtin::DebugPrint,
        emela_core::Builtin::AssertFail => d::Builtin::AssertFail,
    }
}

fn adapt_ctor_ref(source: &emela_core::CtorRef) -> d::CtorRef {
    let emela_core::CtorRef { enum_id, variant } = source;
    d::CtorRef {
        enum_id: id(*enum_id),
        variant: *variant,
    }
}

fn adapt_effect_def(source: &emela_core::EffectDef, type_map: &TypeMap) -> d::EffectDef {
    let emela_core::EffectDef {
        name,
        decl,
        args,
        ops,
        label_of,
    } = source;
    d::EffectDef {
        name: name.clone(),
        decl: decl.clone(),
        args: args.iter().map(|value| type_map.get(*value)).collect(),
        ops: ops
            .iter()
            .map(|item| adapt_op_def(item, type_map))
            .collect(),
        label_of: label_of.as_ref().map(|value| id(*value)),
    }
}

fn adapt_enum_def(source: &emela_core::EnumDef, type_map: &TypeMap) -> d::EnumDef {
    let emela_core::EnumDef {
        name,
        params,
        variants,
        error,
        external,
    } = source;
    d::EnumDef {
        name: name.clone(),
        params: *params,
        variants: variants
            .iter()
            .map(|item| adapt_variant_def(item, type_map))
            .collect(),
        error: *error,
        external: *external,
    }
}

fn adapt_external(source: &emela_core::External) -> d::External {
    let emela_core::External { bindings } = source;
    d::External {
        bindings: bindings.iter().map(adapt_external_binding).collect(),
    }
}

fn adapt_external_binding(source: &emela_core::ExternalBinding) -> d::ExternalBinding {
    let emela_core::ExternalBinding { target, host } = source;
    d::ExternalBinding {
        target: target.clone(),
        host: adapt_host_ref(host),
    }
}

fn adapt_fiber_op(source: &emela_core::FiberOp) -> d::FiberOp {
    match source {
        emela_core::FiberOp::Scope { fails } => d::FiberOp::Scope { fails: *fails },
        emela_core::FiberOp::Fork => d::FiberOp::Fork,
        emela_core::FiberOp::Deferred => d::FiberOp::Deferred,
        emela_core::FiberOp::Join => d::FiberOp::Join,
        emela_core::FiberOp::Await { exit } => d::FiberOp::Await { exit: id(*exit) },
        emela_core::FiberOp::Interrupt => d::FiberOp::Interrupt,
        emela_core::FiberOp::Attempt { exit, suspends } => d::FiberOp::Attempt {
            exit: id(*exit),
            suspends: *suspends,
        },
        emela_core::FiberOp::Complete => d::FiberOp::Complete,
        emela_core::FiberOp::DeferredAwait => d::FiberOp::DeferredAwait,
        emela_core::FiberOp::Poll => d::FiberOp::Poll,
        emela_core::FiberOp::CauseKind { kind } => d::FiberOp::CauseKind { kind: id(*kind) },
        emela_core::FiberOp::CauseToString => d::FiberOp::CauseToString,
        emela_core::FiberOp::CauseRaise { .. } => d::FiberOp::CauseRaise,
    }
}

fn adapt_fn_sets(source: &emela_core::FnSets, type_map: &TypeMap) -> d::FnSets {
    let emela_core::FnSets { errors, effects } = source;
    d::FnSets {
        errors: type_map.get(*errors),
        effects: type_map.get(*effects),
    }
}

fn adapt_fn_type(source: &emela_core::FnType, type_map: &TypeMap) -> d::FnType {
    let emela_core::FnType {
        params,
        ret,
        fails,
        suspends,
        ev,
        sets,
    } = source;
    d::FnType {
        params: params.iter().map(|value| type_map.get(*value)).collect(),
        ret: type_map.get(*ret),
        fails: *fails,
        suspends: *suspends,
        ev: *ev,
        sets: sets.as_ref().map(|item| adapt_fn_sets(item, type_map)),
    }
}

fn adapt_host_module(source: &emela_core::HostModule) -> d::HostModule {
    match source {
        emela_core::HostModule::Named(value0) => d::HostModule::Named(value0.clone()),
        emela_core::HostModule::File(value0) => d::HostModule::File(value0.clone()),
        emela_core::HostModule::Bundled(value0) => d::HostModule::Bundled(value0.clone()),
    }
}

fn adapt_host_ref(source: &emela_core::HostRef) -> d::HostRef {
    match source {
        emela_core::HostRef::Global(value0) => d::HostRef::Global(value0.clone()),
        emela_core::HostRef::Import { module, name } => d::HostRef::Import {
            module: adapt_host_module(module),
            name: name.clone(),
        },
    }
}

fn adapt_op_def(source: &emela_core::OpDef, type_map: &TypeMap) -> d::OpDef {
    let emela_core::OpDef {
        name,
        suspend,
        type_params,
        params,
        ret,
        fails,
    } = source;
    d::OpDef {
        name: name.clone(),
        suspend: *suspend,
        type_params: *type_params,
        params: params.iter().map(|value| type_map.get(*value)).collect(),
        ret: type_map.get(*ret),
        fails: *fails,
    }
}

fn adapt_set_summary(source: &emela_core::SetSummary, type_map: &TypeMap) -> d::SetSummary {
    let emela_core::SetSummary {
        fails,
        suspends,
        ev,
        params,
    } = source;
    d::SetSummary {
        fails: *fails,
        suspends: *suspends,
        ev: *ev,
        params: params.iter().map(|value| type_map.get(*value)).collect(),
    }
}

fn adapt_type(source: &emela_core::Type, type_map: &TypeMap) -> d::Type {
    match source {
        emela_core::Type::Int => d::Type::Int,
        emela_core::Type::Int64 => d::Type::Int64,
        emela_core::Type::Float => d::Type::Float,
        emela_core::Type::Bool => d::Type::Bool,
        emela_core::Type::String => d::Type::String,
        emela_core::Type::Unit => d::Type::Unit,
        emela_core::Type::Never => d::Type::Never,
        emela_core::Type::Tuple(value0) => {
            d::Type::Tuple(value0.iter().map(|value| type_map.get(*value)).collect())
        }
        emela_core::Type::List(value0) => d::Type::List(type_map.get(*value0)),
        emela_core::Type::Enum(value0, value1)
            if Some(*value0) == type_map.bytes && value1.is_empty() =>
        {
            d::Type::Bytes
        }
        emela_core::Type::Enum(value0, value1) => d::Type::Enum(
            id(*value0),
            value1.iter().map(|value| type_map.get(*value)).collect(),
        ),
        emela_core::Type::Param(value0) => d::Type::Param(*value0),
        emela_core::Type::Fn(value0) => d::Type::Fn(adapt_fn_type(value0, type_map)),
        emela_core::Type::Handler(value0) => d::Type::Handler(id(*value0)),
        emela_core::Type::Evidence => d::Type::Evidence,
        emela_core::Type::Cell(value0) => d::Type::Cell(type_map.get(*value0)),
        emela_core::Type::FiberScope => d::Type::FiberScope,
        emela_core::Type::Any => d::Type::Any,
        emela_core::Type::Set(value0) => d::Type::Set(adapt_set_summary(value0, type_map)),
    }
}

fn adapt_variant_def(source: &emela_core::VariantDef, type_map: &TypeMap) -> d::VariantDef {
    let emela_core::VariantDef { name, fields, tys } = source;
    d::VariantDef {
        name: name.clone(),
        fields: adapt_variant_fields(fields),
        tys: tys.iter().map(|value| type_map.get(*value)).collect(),
    }
}

fn adapt_variant_fields(source: &emela_core::VariantFields) -> d::VariantFields {
    match source {
        emela_core::VariantFields::Unit => d::VariantFields::Unit,
        emela_core::VariantFields::Positional(value0) => d::VariantFields::Positional(*value0),
        emela_core::VariantFields::Named(value0) => d::VariantFields::Named(value0.to_vec()),
    }
}

fn adapt_callee(source: &emela_lir::Callee) -> d::Callee {
    match source {
        emela_lir::Callee::Direct(value0) => d::Callee::Direct(id(*value0)),
        emela_lir::Callee::Indirect(value0) => d::Callee::Indirect(id(*value0)),
        emela_lir::Callee::Op {
            handler,
            effect,
            op,
        } => d::Callee::Op {
            handler: id(*handler),
            effect: id(*effect),
            op: *op,
        },
    }
}

fn adapt_case(source: &emela_lir::Case) -> d::Case {
    let emela_lir::Case { key, body } = source;
    d::Case {
        key: adapt_case_key(key),
        body: id(*body),
    }
}

fn adapt_case_key(source: &emela_lir::CaseKey) -> d::CaseKey {
    match source {
        emela_lir::CaseKey::Variant(value0) => d::CaseKey::Variant(*value0),
        emela_lir::CaseKey::Lit(value0) => d::CaseKey::Lit(adapt_const(value0)),
    }
}

fn adapt_closure(source: &emela_lir::Closure) -> d::Closure {
    let emela_lir::Closure {
        params,
        captures,
        body,
        suspends,
    } = source;
    d::Closure {
        params: params.iter().map(|value| id(*value)).collect(),
        captures: captures.iter().map(|value| id(*value)).collect(),
        body: id(*body),
        suspends: *suspends,
    }
}

fn adapt_const(source: &emela_lir::Const) -> d::Const {
    match source {
        emela_lir::Const::Int(value0) => d::Const::Int(*value0),
        emela_lir::Const::Int64(value0) => d::Const::Int64(*value0),
        emela_lir::Const::Float(value0) => d::Const::Float(*value0),
        emela_lir::Const::Bool(value0) => d::Const::Bool(*value0),
        emela_lir::Const::String(value0) => d::Const::String(value0.clone()),
        emela_lir::Const::Unit => d::Const::Unit,
    }
}

fn adapt_function(
    source: &emela_lir::Function,
    core: &mut d::Definitions,
    type_map: &TypeMap,
) -> Result<d::Function, String> {
    if source.env.is_some() || source.defects {
        return Err(format!(
            "function `{}` has metadata from a later IR stage",
            source.name
        ));
    }
    let mut function = d::Function {
        name: source.name.clone(),
        exported: false,
        export_symbol: None,
        params: source.params.iter().map(|value| id(*value)).collect(),
        ret: type_map.get(source.ret),
        fails: source.fails,
        suspends: source.suspends,
        constant: source.constant,
        canonical_const: None,
        external: source.external.as_ref().map(adapt_external),
        env: None,
        body: source.body.map(id),
        vars: arena(&source.vars, |item| adapt_var_data(item, type_map)),
        stmts: Arena::new(),
        seqs: arena(&source.seqs, adapt_seq),
        labels: arena(&source.labels, adapt_label_data),
        // Standard upstream Flow contains ordinary bodies for generated Eq/Show
        // functions; do not infer a private intrinsic from their generated names.
        derivation: None,
        infallible_cause_fail: false,
        statement_positions: HashMap::new(),
        sequence_positions: HashMap::new(),
        active_statements: HashSet::new(),
    };
    for (_, statement) in source.stmts.iter() {
        function
            .stmts
            .alloc(adapt_stmt(statement, source, core, type_map)?);
    }
    Ok(function)
}

fn adapt_label_data(source: &emela_lir::LabelData) -> d::LabelData {
    let emela_lir::LabelData { hint } = source;
    d::LabelData { hint: hint.clone() }
}

fn adapt_on_fail(source: &emela_lir::OnFail) -> d::OnFail {
    match source {
        emela_lir::OnFail::Break(value0) => d::OnFail::Break(id(*value0)),
        emela_lir::OnFail::Propagate => d::OnFail::Propagate,
        emela_lir::OnFail::Handle { error, body } => d::OnFail::Handle {
            error: id(*error),
            body: id(*body),
        },
    }
}

fn adapt_operand(source: &emela_lir::Operand) -> d::Operand {
    match source {
        emela_lir::Operand::Var(value0) => d::Operand::Var(id(*value0)),
        emela_lir::Operand::Const(value0) => d::Operand::Const(adapt_const(value0)),
    }
}

fn adapt_prim_op(source: &emela_lir::PrimOp) -> d::PrimOp {
    match source {
        emela_lir::PrimOp::Add => d::PrimOp::Add,
        emela_lir::PrimOp::Sub => d::PrimOp::Sub,
        emela_lir::PrimOp::Mul => d::PrimOp::Mul,
        emela_lir::PrimOp::Div => d::PrimOp::Div,
        emela_lir::PrimOp::Rem => d::PrimOp::Rem,
        emela_lir::PrimOp::Neg => d::PrimOp::Neg,
        emela_lir::PrimOp::Not => d::PrimOp::Not,
        emela_lir::PrimOp::Eq => d::PrimOp::Eq,
        emela_lir::PrimOp::Ne => d::PrimOp::Ne,
        emela_lir::PrimOp::Lt => d::PrimOp::Lt,
        emela_lir::PrimOp::Le => d::PrimOp::Le,
        emela_lir::PrimOp::Gt => d::PrimOp::Gt,
        emela_lir::PrimOp::Ge => d::PrimOp::Ge,
    }
}

fn adapt_prim_ty(source: &emela_lir::PrimTy) -> d::PrimTy {
    match source {
        emela_lir::PrimTy::I32 => d::PrimTy::I32,
        emela_lir::PrimTy::I64 => d::PrimTy::I64,
        emela_lir::PrimTy::F64 => d::PrimTy::F64,
        emela_lir::PrimTy::String => d::PrimTy::String,
    }
}

fn adapt_runtime_op(source: &emela_lir::RuntimeOp) -> d::RuntimeOp {
    match source {
        emela_lir::RuntimeOp::ShowString => d::RuntimeOp::ShowString,
        emela_lir::RuntimeOp::DebugPrint => d::RuntimeOp::DebugPrint,
        emela_lir::RuntimeOp::Retire => d::RuntimeOp::Retire,
        emela_lir::RuntimeOp::Defect => d::RuntimeOp::Defect,
        emela_lir::RuntimeOp::DefectAt => d::RuntimeOp::DefectAt,
        emela_lir::RuntimeOp::DefectTake => d::RuntimeOp::DefectTake,
        emela_lir::RuntimeOp::DefectResume => d::RuntimeOp::DefectResume,
        emela_lir::RuntimeOp::Hold => d::RuntimeOp::Hold,
        emela_lir::RuntimeOp::Unhold => d::RuntimeOp::Unhold,
        emela_lir::RuntimeOp::Yield => d::RuntimeOp::Yield,
        emela_lir::RuntimeOp::YieldPerform => d::RuntimeOp::YieldPerform,
    }
}

fn adapt_seq(source: &emela_lir::Seq) -> d::Seq {
    let emela_lir::Seq { stmts, term } = source;
    d::Seq {
        stmts: stmts.iter().map(|value| id(*value)).collect(),
        term: adapt_term(term),
    }
}

fn adapt_stmt(
    source: &emela_lir::Stmt,
    function: &emela_lir::Function,
    core: &mut d::Definitions,
    type_map: &TypeMap,
) -> Result<d::Stmt, String> {
    Ok(match source {
        emela_lir::Stmt::Let {
            dst,
            value,
            on_defect,
            on_suspend,
        } => d::Stmt::Let {
            dst: dst.as_ref().map(|value| id(*value)),
            value: adapt_value(value, *dst, function, core, type_map)?,
            on_defect: on_defect.as_ref().map(|value| id(*value)),
            on_suspend: on_suspend.as_ref().map(adapt_suspend),
        },
        emela_lir::Stmt::If {
            dst,
            cond,
            then,
            else_,
        } => d::Stmt::If {
            dst: dst.as_ref().map(|value| id(*value)),
            cond: adapt_operand(cond),
            then: id(*then),
            else_: id(*else_),
        },
        emela_lir::Stmt::Switch {
            dst,
            on,
            cases,
            default,
        } => d::Stmt::Switch {
            dst: dst.as_ref().map(|value| id(*value)),
            on: adapt_switch_on(on),
            cases: cases.iter().map(adapt_case).collect(),
            default: default.as_ref().map(|value| id(*value)),
        },
        emela_lir::Stmt::Loop {
            dst,
            vars,
            inits,
            body,
        } => d::Stmt::Loop {
            dst: dst.as_ref().map(|value| id(*value)),
            vars: vars.iter().map(|value| id(*value)).collect(),
            inits: inits.iter().map(adapt_operand).collect(),
            body: id(*body),
        },
        emela_lir::Stmt::Block { dst, label, body } => d::Stmt::Block {
            dst: dst.as_ref().map(|value| id(*value)),
            label: id(*label),
            body: id(*body),
        },
        emela_lir::Stmt::Protect { dst, body, cleanup } => d::Stmt::Protect {
            dst: dst.as_ref().map(|value| id(*value)),
            body: id(*body),
            cleanup: id(*cleanup),
        },
        emela_lir::Stmt::Dup(value0) => d::Stmt::Dup(id(*value0)),
        emela_lir::Stmt::Drop(value0) => d::Stmt::Drop(id(*value0)),
    })
}

fn adapt_suspend(source: &emela_lir::Suspend) -> d::Suspend {
    let emela_lir::Suspend { state, saved } = source;
    d::Suspend {
        state: *state,
        saved: saved.iter().map(|value| id(*value)).collect(),
    }
}

fn adapt_switch_on(source: &emela_lir::SwitchOn) -> d::SwitchOn {
    match source {
        emela_lir::SwitchOn::Tag(value0) => d::SwitchOn::Tag(id(*value0)),
        emela_lir::SwitchOn::Lit(value0) => d::SwitchOn::Lit(id(*value0)),
    }
}

fn adapt_term(source: &emela_lir::Term) -> d::Term {
    match source {
        emela_lir::Term::Yield(value0) => d::Term::Yield(value0.as_ref().map(adapt_operand)),
        emela_lir::Term::Return(value0) => d::Term::Return(value0.as_ref().map(adapt_operand)),
        emela_lir::Term::Fail(value0) => d::Term::Fail(adapt_operand(value0)),
        emela_lir::Term::Break { label, value } => d::Term::Break {
            label: id(*label),
            value: value.as_ref().map(adapt_operand),
        },
        emela_lir::Term::Recur(value0) => {
            d::Term::Recur(value0.iter().map(adapt_operand).collect())
        }
        emela_lir::Term::Defect(value0, _) => d::Term::Defect(adapt_operand(value0)),
        emela_lir::Term::Unwind => d::Term::Unwind,
        emela_lir::Term::Unreachable => d::Term::Unreachable,
    }
}

fn adapt_value(
    source: &emela_lir::Value,
    dst: Option<emela_lir::VarId>,
    function: &emela_lir::Function,
    core: &mut d::Definitions,
    type_map: &TypeMap,
) -> Result<d::Value, String> {
    Ok(match source {
        emela_lir::Value::Const(value0) => d::Value::Const(adapt_const(value0)),
        emela_lir::Value::Prim { op, ty, args } => d::Value::Prim {
            op: adapt_prim_op(op),
            ty: adapt_prim_ty(ty),
            args: args.iter().map(adapt_operand).collect(),
        },
        emela_lir::Value::Call {
            callee,
            args,
            suspends,
            on_fail,
        } => d::Value::Call {
            callee: adapt_callee(callee),
            args: args.iter().map(adapt_operand).collect(),
            suspends: *suspends,
            on_fail: on_fail.as_ref().map(adapt_on_fail),
        },
        emela_lir::Value::Builtin { op, args } => d::Value::Builtin {
            op: adapt_builtin(op),
            args: args.iter().map(adapt_operand).collect(),
        },
        emela_lir::Value::Make { ctor, args } => d::Value::Make {
            ctor: adapt_ctor_ref(ctor),
            args: args.iter().map(adapt_operand).collect(),
        },
        emela_lir::Value::Field { base, ctor, index } => d::Value::Field {
            base: id(*base),
            ctor: adapt_ctor_ref(ctor),
            index: *index,
        },
        emela_lir::Value::Tag(value0) => d::Value::Tag(id(*value0)),
        emela_lir::Value::Tuple(value0) => {
            d::Value::Tuple(value0.iter().map(adapt_operand).collect())
        }
        emela_lir::Value::TupleGet { base, index } => d::Value::TupleGet {
            base: id(*base),
            index: *index,
        },
        emela_lir::Value::List { elems, tail } => d::Value::List {
            elems: elems.iter().map(adapt_operand).collect(),
            tail: tail.as_ref().map(adapt_operand),
        },
        emela_lir::Value::ListIsEmpty(value0) => d::Value::ListIsEmpty(id(*value0)),
        emela_lir::Value::ListHead(value0) => d::Value::ListHead(id(*value0)),
        emela_lir::Value::ListTail(value0) => d::Value::ListTail(id(*value0)),
        emela_lir::Value::FnRef(value0) => d::Value::FnRef(id(*value0)),
        emela_lir::Value::Closure(value0) => d::Value::Closure(Box::new(adapt_closure(value0))),
        emela_lir::Value::MakeClosure { function, captures } => d::Value::MakeClosure {
            function: id(*function),
            captures: captures.iter().map(|value| id(*value)).collect(),
        },
        emela_lir::Value::EnvGet { env, index } => d::Value::EnvGet {
            env: id(*env),
            index: *index,
        },
        emela_lir::Value::EvLookup { ev, effect } => d::Value::EvLookup {
            ev: id(*ev),
            effect: id(*effect),
        },
        emela_lir::Value::EvExtend { base, bindings } => d::Value::EvExtend {
            base: base.as_ref().map(|value| id(*value)),
            bindings: bindings
                .iter()
                .map(|value| (id(value.0), id(value.1)))
                .collect(),
        },
        emela_lir::Value::Handler { name, effect, ops } => d::Value::Handler {
            name: name.clone(),
            effect: id(*effect),
            ops: ops.iter().map(|value| id(*value)).collect(),
        },
        emela_lir::Value::Fiber { op, args, on_fail } => d::Value::Fiber {
            op: adapt_fiber_op(op),
            args: args.iter().map(adapt_operand).collect(),
            on_fail: on_fail.as_ref().map(adapt_on_fail),
        },
        emela_lir::Value::IsError { error, enum_id } => d::Value::IsError {
            error: id(*error),
            enum_id: id(*enum_id),
            error_type: error_type(core, function, *enum_id, None, type_map)?,
        },
        emela_lir::Value::AsError { error, enum_id } => d::Value::AsError {
            error: id(*error),
            enum_id: id(*enum_id),
            error_type: error_type(core, function, *enum_id, dst, type_map)?,
        },
        emela_lir::Value::Runtime { op, args } => d::Value::Runtime {
            op: adapt_runtime_op(op),
            args: args.iter().map(adapt_operand).collect(),
        },
        emela_lir::Value::Coerce(value0) => d::Value::Coerce(id(*value0)),
    })
}

fn adapt_var_data(source: &emela_lir::VarData, type_map: &TypeMap) -> d::VarData {
    let emela_lir::VarData { hint, repr, ty } = source;
    d::VarData {
        hint: hint.clone(),
        repr: adapt_repr(repr),
        ty: type_map.get(*ty),
    }
}

fn adapt_repr(source: &emela_lir::Repr) -> d::Repr {
    match source {
        emela_lir::Repr::I32 => d::Repr::I32,
        emela_lir::Repr::I64 => d::Repr::I64,
        emela_lir::Repr::F64 => d::Repr::F64,
        emela_lir::Repr::Ref => d::Repr::Ref,
        emela_lir::Repr::Void => d::Repr::Void,
    }
}

// Flow error operations identify their enum. Monomorphic upstream error
// declarations have no parameters; AsError also carries its result's type.
fn error_type(
    core: &mut d::Definitions,
    function: &emela_lir::Function,
    enum_id: emela_core::EnumId,
    dst: Option<emela_lir::VarId>,
    type_map: &TypeMap,
) -> Result<d::TypeId, String> {
    let converted = id(enum_id);
    if let Some(dst) = dst {
        let ty = type_map.get(function.vars[dst].ty);
        if matches!(core.ty(ty), d::Type::Enum(candidate, _) if *candidate == converted) {
            return Ok(ty);
        }
    }
    if core.enums[converted].params == 0 {
        return Ok(core.types.intern(d::Type::Enum(converted, vec![])));
    }
    Err(format!(
        "function `{}`: Flow error operation on generic enum `{}` has no concrete type metadata",
        function.name, core.enums[converted].name,
    ))
}

#[cfg(test)]
mod tests;
