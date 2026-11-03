//! Canonical backend values and definitions, owned independently of Core and Flow IR.
//! Frontend function bodies, name-resolution state, source files, and Heap-IR stages are absent.
//! The upstream adapter constructs these owned definitions; downstream stages use these IDs only.
use crate::contract::DerivedOperation;
use crate::input::codec;
use la_arena::{Arena, Idx};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub type FnId = Idx<Function>;
pub type VarId = Idx<VarData>;
pub type StmtId = Idx<Stmt>;
pub type SeqId = Idx<Seq>;
pub type LabelId = Idx<LabelData>;
pub type EnumId = Idx<EnumDef>;
pub type EffectId = Idx<EffectDef>;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definitions {
    pub types: TypeTable,
    #[serde(with = "codec::arena")]
    pub enums: Arena<EnumDef>,
    #[serde(with = "codec::arena")]
    pub effects: Arena<EffectDef>,
    #[serde(skip)]
    pub expanding_enums: HashSet<EnumId>,
}
impl Definitions {
    pub fn ty(&self, ty: TypeId) -> &Type {
        &self.types[ty]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Program {
    pub core: Definitions,
    #[serde(with = "codec::arena")]
    pub functions: Arena<Function>,
    #[serde(with = "codec::index_pairs")]
    pub error_shows: Vec<(EnumId, FnId)>,
    #[serde(with = "codec::index_groups")]
    pub const_function_groups: Vec<Vec<FnId>>,
    #[serde(skip)]
    pub has_scopes: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Function {
    pub name: String,
    pub exported: bool,
    /// Explicit public C name supplied by the adapter's export configuration.
    pub export_symbol: Option<String>,
    #[serde(with = "codec::indices")]
    pub params: Vec<VarId>,
    pub ret: TypeId,
    pub fails: bool,
    pub suspends: bool,
    pub constant: bool,
    #[serde(with = "codec::optional_index")]
    pub canonical_const: Option<FnId>,
    pub external: Option<External>,
    pub env: Option<Vec<TypeId>>,
    #[serde(with = "codec::optional_index")]
    pub body: Option<SeqId>,
    #[serde(with = "codec::arena")]
    pub vars: Arena<VarData>,
    #[serde(with = "codec::arena")]
    pub stmts: Arena<Stmt>,
    #[serde(with = "codec::arena")]
    pub seqs: Arena<Seq>,
    #[serde(with = "codec::arena")]
    pub labels: Arena<LabelData>,
    pub derivation: Option<DerivedOperation>,
    pub infallible_cause_fail: bool,
    #[serde(skip)]
    pub statement_positions: HashMap<StmtId, usize>,
    #[serde(skip)]
    pub sequence_positions: HashMap<SeqId, usize>,
    #[serde(skip)]
    pub active_statements: HashSet<StmtId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VarData {
    pub hint: String,
    pub repr: Repr,
    pub ty: TypeId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabelData {
    pub hint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Seq {
    #[serde(with = "codec::indices")]
    pub stmts: Vec<StmtId>,
    pub term: Term,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Term {
    Yield(Option<Operand>),
    Return(Option<Operand>),
    Fail(Operand),
    Break {
        #[serde(with = "codec::index")]
        label: LabelId,
        value: Option<Operand>,
    },
    Recur(Vec<Operand>),
    Defect(Operand),
    Unwind,
    Unreachable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Stmt {
    Let {
        #[serde(with = "codec::optional_index")]
        dst: Option<VarId>,
        value: Value,
        #[serde(with = "codec::optional_index")]
        on_defect: Option<SeqId>,
        on_suspend: Option<Suspend>,
    },
    If {
        #[serde(with = "codec::optional_index")]
        dst: Option<VarId>,
        cond: Operand,
        #[serde(with = "codec::index")]
        then: SeqId,
        #[serde(with = "codec::index")]
        else_: SeqId,
    },
    Switch {
        #[serde(with = "codec::optional_index")]
        dst: Option<VarId>,
        on: SwitchOn,
        cases: Vec<Case>,
        #[serde(with = "codec::optional_index")]
        default: Option<SeqId>,
    },
    Loop {
        #[serde(with = "codec::optional_index")]
        dst: Option<VarId>,
        #[serde(with = "codec::indices")]
        vars: Vec<VarId>,
        inits: Vec<Operand>,
        #[serde(with = "codec::index")]
        body: SeqId,
    },
    Block {
        #[serde(with = "codec::optional_index")]
        dst: Option<VarId>,
        #[serde(with = "codec::index")]
        label: LabelId,
        #[serde(with = "codec::index")]
        body: SeqId,
    },
    Protect {
        #[serde(with = "codec::optional_index")]
        dst: Option<VarId>,
        #[serde(with = "codec::index")]
        body: SeqId,
        #[serde(with = "codec::index")]
        cleanup: SeqId,
    },
    Dup(#[serde(with = "codec::index")] VarId),
    Drop(#[serde(with = "codec::index")] VarId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Suspend {
    pub state: u32,
    #[serde(with = "codec::indices")]
    pub saved: Vec<VarId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SwitchOn {
    Tag(#[serde(with = "codec::index")] VarId),
    Lit(#[serde(with = "codec::index")] VarId),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub key: CaseKey,
    #[serde(with = "codec::index")]
    pub body: SeqId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum CaseKey {
    Variant(usize),
    Lit(Const),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Operand {
    Var(#[serde(with = "codec::index")] VarId),
    Const(Const),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Const {
    Int(i32),
    Int64(i64),
    Float(#[serde(with = "codec::float_bits")] f64),
    Bool(bool),
    String(String),
    Unit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Value {
    Const(Const),
    Prim {
        op: PrimOp,
        ty: PrimTy,
        args: Vec<Operand>,
    },
    Call {
        callee: Callee,
        args: Vec<Operand>,
        suspends: bool,
        on_fail: Option<OnFail>,
    },
    Builtin {
        op: Builtin,
        args: Vec<Operand>,
    },
    Make {
        ctor: CtorRef,
        args: Vec<Operand>,
    },
    Field {
        #[serde(with = "codec::index")]
        base: VarId,
        ctor: CtorRef,
        index: usize,
    },
    Tag(#[serde(with = "codec::index")] VarId),
    Tuple(Vec<Operand>),
    TupleGet {
        #[serde(with = "codec::index")]
        base: VarId,
        index: usize,
    },
    List {
        elems: Vec<Operand>,
        tail: Option<Operand>,
    },
    ListIsEmpty(#[serde(with = "codec::index")] VarId),
    ListHead(#[serde(with = "codec::index")] VarId),
    ListTail(#[serde(with = "codec::index")] VarId),
    FnRef(#[serde(with = "codec::index")] FnId),
    Closure(Box<Closure>),
    MakeClosure {
        #[serde(with = "codec::index")]
        function: FnId,
        #[serde(with = "codec::indices")]
        captures: Vec<VarId>,
    },
    EnvGet {
        #[serde(with = "codec::index")]
        env: VarId,
        index: usize,
    },
    EvLookup {
        #[serde(with = "codec::index")]
        ev: VarId,
        #[serde(with = "codec::index")]
        effect: EffectId,
    },
    EvExtend {
        #[serde(with = "codec::optional_index")]
        base: Option<VarId>,
        #[serde(with = "codec::index_pairs")]
        bindings: Vec<(EffectId, VarId)>,
    },
    Handler {
        name: String,
        #[serde(with = "codec::index")]
        effect: EffectId,
        #[serde(with = "codec::indices")]
        ops: Vec<VarId>,
    },
    Fiber {
        op: FiberOp,
        args: Vec<Operand>,
        on_fail: Option<OnFail>,
    },
    IsError {
        #[serde(with = "codec::index")]
        error: VarId,
        #[serde(with = "codec::index")]
        enum_id: EnumId,
        error_type: TypeId,
    },
    AsError {
        #[serde(with = "codec::index")]
        error: VarId,
        #[serde(with = "codec::index")]
        enum_id: EnumId,
        error_type: TypeId,
    },
    Runtime {
        op: RuntimeOp,
        args: Vec<Operand>,
    },
    Coerce(#[serde(with = "codec::index")] VarId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum RuntimeOp {
    ShowString,
    DebugPrint,
    Retire,
    Defect,
    DefectAt,
    DefectTake,
    DefectResume,
    Hold,
    Unhold,
    Yield,
    YieldPerform,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Closure {
    #[serde(with = "codec::indices")]
    pub params: Vec<VarId>,
    #[serde(with = "codec::indices")]
    pub captures: Vec<VarId>,
    #[serde(with = "codec::index")]
    pub body: SeqId,
    pub suspends: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Callee {
    Direct(#[serde(with = "codec::index")] FnId),
    Indirect(#[serde(with = "codec::index")] VarId),
    Op {
        #[serde(with = "codec::index")]
        handler: VarId,
        #[serde(with = "codec::index")]
        effect: EffectId,
        op: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum OnFail {
    Break(#[serde(with = "codec::index")] LabelId),
    Propagate,
    Handle {
        #[serde(with = "codec::index")]
        error: VarId,
        #[serde(with = "codec::index")]
        body: SeqId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum PrimOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Neg,
    Not,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum PrimTy {
    I32,
    I64,
    F64,
    String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct External {
    pub bindings: Vec<ExternalBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalBinding {
    pub target: String,
    pub host: HostRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum HostRef {
    Global(String),
    Import { module: HostModule, name: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum HostModule {
    Named(String),
    File(String),
    Bundled(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectDef {
    pub name: String,
    pub decl: String,
    pub args: Vec<TypeId>,
    pub ops: Vec<OpDef>,
    #[serde(with = "codec::optional_index")]
    pub label_of: Option<EffectId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpDef {
    pub name: String,
    pub suspend: bool,
    pub type_params: usize,
    pub params: Vec<TypeId>,
    pub ret: TypeId,
    pub fails: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnumDef {
    pub name: String,
    pub params: usize,
    pub variants: Vec<VariantDef>,
    pub error: bool,
    pub external: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VariantDef {
    pub name: String,
    pub fields: VariantFields,
    pub tys: Vec<TypeId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum VariantFields {
    Unit,
    Positional(usize),
    Named(Vec<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CtorRef {
    #[serde(with = "codec::index")]
    pub enum_id: EnumId,
    pub variant: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeId(u32);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Type {
    Int,
    Int64,
    Float,
    Bool,
    String,
    Bytes,
    Unit,
    Never,
    Tuple(Vec<TypeId>),
    List(TypeId),
    Enum(#[serde(with = "codec::index")] EnumId, Vec<TypeId>),
    Param(usize),
    Fn(FnType),
    Handler(#[serde(with = "codec::index")] EffectId),
    Evidence,
    Cell(TypeId),
    FiberScope,
    Any,
    Set(SetSummary),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetSummary {
    pub fails: bool,
    pub suspends: bool,
    pub ev: bool,
    pub params: Vec<TypeId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FnSets {
    pub errors: TypeId,
    pub effects: TypeId,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FnType {
    pub params: Vec<TypeId>,
    pub ret: TypeId,
    pub fails: bool,
    pub suspends: bool,
    pub ev: bool,
    pub sets: Option<FnSets>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum FiberOp {
    Scope {
        fails: bool,
    },
    Fork,
    Deferred,
    Join,
    Await {
        #[serde(with = "codec::index")]
        exit: EnumId,
    },
    Interrupt,
    Attempt {
        #[serde(with = "codec::index")]
        exit: EnumId,
        suspends: bool,
    },
    Complete,
    DeferredAwait,
    Poll,
    CauseKind {
        #[serde(with = "codec::index")]
        kind: EnumId,
    },
    CauseToString,
    CauseRaise,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeTable {
    pub types: Vec<Type>,
    #[serde(skip)]
    pub has_param: Vec<bool>,
    #[serde(skip)]
    pub ids: HashMap<Type, TypeId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Builtin {
    StringLength,
    StringByteSize,
    StringAppend,
    StringContains,
    StringStartsWith,
    StringEndsWith,
    StringSplit,
    StringChars,
    StringJoin,
    StringTrim,
    StringCodePoints,
    StringFromCodePoint,
    ShowNested,
    IntToString,
    IntParse,
    IntCheckedAdd,
    IntBitAnd,
    IntBitOr,
    IntBitXor,
    IntBitNot,
    IntShiftLeft,
    IntShiftRight,
    IntShiftRightUnsigned,
    Int64FromInt,
    Int64ToString,
    Int64ToInt,
    Int64BitAnd,
    Int64BitOr,
    Int64BitXor,
    Int64BitNot,
    Int64ShiftLeft,
    Int64ShiftRight,
    Int64ShiftRightUnsigned,
    FloatToString,
    FloatFromInt,
    FloatParse,
    FloatFloor,
    FloatCeil,
    FloatRound,
    FloatTruncate,
    BytesFromList,
    BytesToList,
    BytesSize,
    BytesGet,
    BytesSlice,
    BytesConcat,
    BytesEncodeUtf8,
    BytesDecodeValidUtf8,
    Panic,
    AssertFail,
    Todo,
    Dbg,
    Inspect,
    CellNew,
    CellGet,
    CellSet,
    ShowString,
    DebugPrint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Repr {
    I32,
    I64,
    F64,
    Ref,
    Void,
}

impl TypeId {
    pub const INT: TypeId = TypeId(0);
    pub const INT64: TypeId = TypeId(1);
    pub const FLOAT: TypeId = TypeId(2);
    pub const BOOL: TypeId = TypeId(3);
    pub const STRING: TypeId = TypeId(4);
    pub const UNIT: TypeId = TypeId(5);
    pub const NEVER: TypeId = TypeId(6);
    pub const EVIDENCE: TypeId = TypeId(7);
    pub const FIBER_SCOPE: TypeId = TypeId(8);
    pub const ANY: TypeId = TypeId(9);
    pub const BYTES: TypeId = TypeId(10);
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

impl std::fmt::Debug for TypeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TypeId({})", self.0)
    }
}

impl VariantFields {
    pub fn len(&self) -> usize {
        match self {
            VariantFields::Unit => 0,
            VariantFields::Positional(n) => *n,
            VariantFields::Named(names) => names.len(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl FnType {}

impl External {
    pub fn binding(&self, target: &str) -> Option<&ExternalBinding> {
        self.bindings.iter().find(|b| b.target == target)
    }
}

impl std::ops::Index<TypeId> for TypeTable {
    type Output = Type;
    fn index(&self, id: TypeId) -> &Type {
        &self.types[id.index()]
    }
}

impl Stmt {
    pub fn dst(&self) -> Option<VarId> {
        match self {
            Stmt::Let { dst, .. }
            | Stmt::If { dst, .. }
            | Stmt::Switch { dst, .. }
            | Stmt::Loop { dst, .. }
            | Stmt::Block { dst, .. }
            | Stmt::Protect { dst, .. } => *dst,
            Stmt::Dup(_) | Stmt::Drop(_) => None,
        }
    }
}

impl Const {}

impl PrimOp {}

impl PrimTy {}

impl RuntimeOp {}

impl FiberOp {}

impl Repr {
    pub fn name(self) -> &'static str {
        match self {
            Repr::I32 => "i32",
            Repr::I64 => "i64",
            Repr::F64 => "f64",
            Repr::Ref => "ref",
            Repr::Void => "void",
        }
    }
}

impl std::fmt::Display for Repr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

impl Value {
    pub fn for_each_var_mut(&mut self, mut f: impl FnMut(&mut VarId)) {
        fn operands(values: &mut [Operand], f: &mut impl FnMut(&mut VarId)) {
            for value in values {
                if let Operand::Var(variable) = value {
                    f(variable);
                }
            }
        }
        match self {
            Value::Const(_) | Value::FnRef(_) => {}
            Value::Prim { args, .. }
            | Value::Builtin { args, .. }
            | Value::Make { args, .. }
            | Value::Tuple(args)
            | Value::Fiber { args, .. }
            | Value::Runtime { args, .. } => operands(args, &mut f),
            Value::Call { callee, args, .. } => {
                match callee {
                    Callee::Direct(_) => {}
                    Callee::Indirect(v) | Callee::Op { handler: v, .. } => f(v),
                }
                operands(args, &mut f);
            }
            Value::Field { base: v, .. }
            | Value::Tag(v)
            | Value::TupleGet { base: v, .. }
            | Value::ListIsEmpty(v)
            | Value::ListHead(v)
            | Value::ListTail(v)
            | Value::EvLookup { ev: v, .. }
            | Value::IsError { error: v, .. }
            | Value::AsError { error: v, .. }
            | Value::Coerce(v)
            | Value::EnvGet { env: v, .. } => f(v),
            Value::List { elems, tail } => {
                operands(elems, &mut f);
                if let Some(Operand::Var(v)) = tail {
                    f(v);
                }
            }
            Value::Closure(c) => c.captures.iter_mut().for_each(f),
            Value::MakeClosure { captures, .. } => captures.iter_mut().for_each(f),
            Value::EvExtend { base, bindings } => {
                if let Some(v) = base {
                    f(v);
                }
                for (_, v) in bindings {
                    f(v);
                }
            }
            Value::Handler { ops, .. } => ops.iter_mut().for_each(f),
        }
    }
    pub fn for_each_var(&self, mut f: impl FnMut(VarId)) {
        let ops = |os: &[Operand], f: &mut dyn FnMut(VarId)| {
            for o in os {
                if let Operand::Var(v) = o {
                    f(*v);
                }
            }
        };
        match self {
            Value::Const(_) | Value::FnRef(_) => {}
            Value::Prim { args, .. }
            | Value::Builtin { args, .. }
            | Value::Make { args, .. }
            | Value::Tuple(args)
            | Value::Fiber { args, .. }
            | Value::Runtime { args, .. } => ops(args, &mut f),
            Value::Call { callee, args, .. } => {
                match callee {
                    Callee::Direct(_) => {}
                    Callee::Indirect(v) | Callee::Op { handler: v, .. } => f(*v),
                }
                ops(args, &mut f);
            }
            Value::Field { base: v, .. }
            | Value::Tag(v)
            | Value::TupleGet { base: v, .. }
            | Value::ListIsEmpty(v)
            | Value::ListHead(v)
            | Value::ListTail(v)
            | Value::EvLookup { ev: v, .. }
            | Value::IsError { error: v, .. }
            | Value::AsError { error: v, .. }
            | Value::Coerce(v) => f(*v),
            Value::List { elems, tail } => {
                ops(elems, &mut f);
                ops(tail.as_slice(), &mut f);
            }
            Value::Closure(c) => c.captures.iter().for_each(|&v| f(v)),
            Value::MakeClosure { captures, .. } => captures.iter().for_each(|&v| f(v)),
            Value::EnvGet { env, .. } => f(*env),
            Value::EvExtend { base, bindings } => {
                base.iter().for_each(|&v| f(v));
                bindings.iter().for_each(|&(_, v)| f(v));
            }
            Value::Handler { ops: hs, .. } => hs.iter().for_each(|&v| f(v)),
        }
    }
}

impl From<VarId> for Operand {
    fn from(value: VarId) -> Self {
        Self::Var(value)
    }
}
impl From<Const> for Operand {
    fn from(value: Const) -> Self {
        Self::Const(value)
    }
}

impl TypeId {
    pub fn from_index(index: usize) -> Self {
        Self(u32::try_from(index).expect("backend type table exceeds u32"))
    }
}
pub const EXIT_SUCCESS: usize = 0;
pub const EXIT_FAILURE: usize = 1;
pub const CAUSE_FAIL: usize = 0;
pub const CAUSE_INTERRUPT: usize = 2;

impl Function {
    pub fn new(name: &str) -> Self {
        Function {
            derivation: None,
            infallible_cause_fail: false,
            statement_positions: HashMap::new(),
            sequence_positions: HashMap::new(),
            active_statements: HashSet::new(),
            name: name.to_owned(),
            exported: false,
            export_symbol: None,
            params: Vec::new(),
            ret: TypeId::UNIT,
            fails: false,
            suspends: false,
            constant: false,
            canonical_const: None,
            external: None,
            env: None,
            body: None,
            vars: Arena::new(),
            stmts: Arena::new(),
            seqs: Arena::new(),
            labels: Arena::new(),
        }
    }
}
