//! Canonical backend values and definitions, owned independently of Core and Flow IR.
//! Frontend function bodies, name-resolution state, source files, and Heap-IR stages are absent.
//! The independent frontend constructs these owned definitions; downstream stages use these IDs only.
use crate::contract::DerivedOperation;
use la_arena::{Arena, Idx};
use std::collections::{HashMap, HashSet};

pub type FnId = Idx<Function>;
pub type VarId = Idx<VarData>;
pub type StmtId = Idx<Stmt>;
pub type SeqId = Idx<Seq>;
pub type LabelId = Idx<LabelData>;
pub type EnumId = Idx<EnumDef>;
pub type EffectId = Idx<EffectDef>;

#[derive(Debug, Clone, Default)]
pub struct Definitions {
    pub types: TypeTable,
    pub enums: Arena<EnumDef>,
    pub effects: Arena<EffectDef>,
    pub expanding_enums: HashSet<EnumId>,
}
impl Definitions {
    pub fn ty(&self, ty: TypeId) -> &Type {
        &self.types[ty]
    }
}

#[derive(Debug, Clone)]
pub struct Program {
    pub core: Definitions,
    pub functions: Arena<Function>,
    pub error_shows: Vec<(EnumId, FnId)>,
    pub const_function_groups: Vec<Vec<FnId>>,
    pub has_scopes: bool,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub exported: bool,
    /// Explicit public C name supplied by the independent frontend.
    pub export_symbol: Option<String>,
    pub params: Vec<VarId>,
    pub ret: TypeId,
    pub fails: bool,
    pub suspends: bool,
    pub constant: bool,
    pub canonical_const: Option<FnId>,
    pub external: Option<External>,
    pub env: Option<Vec<TypeId>>,
    pub body: Option<SeqId>,
    pub vars: Arena<VarData>,
    pub stmts: Arena<Stmt>,
    pub seqs: Arena<Seq>,
    pub labels: Arena<LabelData>,
    pub derivation: Option<DerivedOperation>,
    pub infallible_cause_fail: bool,
    pub statement_positions: HashMap<StmtId, usize>,
    pub sequence_positions: HashMap<SeqId, usize>,
    pub active_statements: HashSet<StmtId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarData {
    pub hint: String,
    pub repr: Repr,
    pub ty: TypeId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelData {
    pub hint: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Seq {
    pub stmts: Vec<StmtId>,
    pub term: Term,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Term {
    Yield(Option<Operand>),
    Return(Option<Operand>),
    Fail(Operand),
    Break {
        label: LabelId,
        value: Option<Operand>,
    },
    Recur(Vec<Operand>),
    Defect(Operand),
    Unwind,
    Unreachable,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Let {
        dst: Option<VarId>,
        value: Value,
        on_defect: Option<SeqId>,
        on_suspend: Option<Suspend>,
    },
    If {
        dst: Option<VarId>,
        cond: Operand,
        then: SeqId,
        else_: SeqId,
    },
    Switch {
        dst: Option<VarId>,
        on: SwitchOn,
        cases: Vec<Case>,
        default: Option<SeqId>,
    },
    Loop {
        dst: Option<VarId>,
        vars: Vec<VarId>,
        inits: Vec<Operand>,
        body: SeqId,
    },
    Block {
        dst: Option<VarId>,
        label: LabelId,
        body: SeqId,
    },
    Protect {
        dst: Option<VarId>,
        body: SeqId,
        cleanup: SeqId,
    },
    Dup(VarId),
    Drop(VarId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suspend {
    pub state: u32,
    pub saved: Vec<VarId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchOn {
    Tag(VarId),
    Lit(VarId),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Case {
    pub key: CaseKey,
    pub body: SeqId,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CaseKey {
    Variant(usize),
    Lit(Const),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    Var(VarId),
    Const(Const),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Const {
    Int(i32),
    Int64(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Unit,
}

#[derive(Debug, Clone, PartialEq)]
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
        base: VarId,
        ctor: CtorRef,
        index: usize,
    },
    Tag(VarId),
    Tuple(Vec<Operand>),
    TupleGet {
        base: VarId,
        index: usize,
    },
    List {
        elems: Vec<Operand>,
        tail: Option<Operand>,
    },
    ListIsEmpty(VarId),
    ListHead(VarId),
    ListTail(VarId),
    FnRef(FnId),
    Closure(Box<Closure>),
    MakeClosure {
        function: FnId,
        captures: Vec<VarId>,
    },
    EnvGet {
        env: VarId,
        index: usize,
    },
    EvLookup {
        ev: VarId,
        effect: EffectId,
    },
    EvExtend {
        base: Option<VarId>,
        bindings: Vec<(EffectId, VarId)>,
    },
    Handler {
        name: String,
        effect: EffectId,
        ops: Vec<VarId>,
    },
    Fiber {
        op: FiberOp,
        args: Vec<Operand>,
        on_fail: Option<OnFail>,
    },
    IsError {
        error: VarId,
        enum_id: EnumId,
        error_type: TypeId,
    },
    AsError {
        error: VarId,
        enum_id: EnumId,
        error_type: TypeId,
    },
    Runtime {
        op: RuntimeOp,
        args: Vec<Operand>,
    },
    Coerce(VarId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq)]
pub struct Closure {
    pub params: Vec<VarId>,
    pub captures: Vec<VarId>,
    pub body: SeqId,
    pub suspends: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Callee {
    Direct(FnId),
    Indirect(VarId),
    Op {
        handler: VarId,
        effect: EffectId,
        op: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnFail {
    Break(LabelId),
    Propagate,
    Handle { error: VarId, body: SeqId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimTy {
    I32,
    I64,
    F64,
    String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct External {
    pub bindings: Vec<ExternalBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalBinding {
    pub target: String,
    pub host: HostRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostRef {
    Global(String),
    Import { module: HostModule, name: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostModule {
    Named(String),
    File(String),
    Bundled(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectDef {
    pub name: String,
    pub decl: String,
    pub args: Vec<TypeId>,
    pub ops: Vec<OpDef>,
    pub label_of: Option<EffectId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpDef {
    pub name: String,
    pub suspend: bool,
    pub type_params: usize,
    pub params: Vec<TypeId>,
    pub ret: TypeId,
    pub fails: bool,
}

#[derive(Debug, Clone)]
pub struct EnumDef {
    pub name: String,
    pub params: usize,
    pub variants: Vec<VariantDef>,
    pub error: bool,
    pub external: bool,
}

#[derive(Debug, Clone)]
pub struct VariantDef {
    pub name: String,
    pub fields: VariantFields,
    pub tys: Vec<TypeId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VariantFields {
    Unit,
    Positional(usize),
    Named(Vec<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CtorRef {
    pub enum_id: EnumId,
    pub variant: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TypeId(u32);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
    Enum(EnumId, Vec<TypeId>),
    Param(usize),
    Fn(FnType),
    Handler(EffectId),
    Evidence,
    Cell(TypeId),
    FiberScope,
    Any,
    Set(SetSummary),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct SetSummary {
    pub fails: bool,
    pub suspends: bool,
    pub ev: bool,
    pub params: Vec<TypeId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FnSets {
    pub errors: TypeId,
    pub effects: TypeId,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FnType {
    pub params: Vec<TypeId>,
    pub ret: TypeId,
    pub fails: bool,
    pub suspends: bool,
    pub ev: bool,
    pub sets: Option<FnSets>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FiberOp {
    Scope { fails: bool },
    Fork,
    Deferred,
    Join,
    Await { exit: EnumId },
    Interrupt,
    Attempt { exit: EnumId, suspends: bool },
    Complete,
    DeferredAwait,
    Poll,
    CauseKind { kind: EnumId },
    CauseToString,
    CauseRaise,
}

#[derive(Debug, Clone)]
pub struct TypeTable {
    pub types: Vec<Type>,
    pub has_param: Vec<bool>,
    pub ids: HashMap<Type, TypeId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
