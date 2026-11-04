//! Call-site shorthand for the bundled Embedded interfaces.
//!
//! Upstream still owns parsing, name resolution, type inference and lowering.
//! Constructor defaults use resolved definition IDs; operation defaults use the
//! upstream checker's capability-operation table. Defaults never follow a name
//! through a function value, and do not change declarations or the C ABI.

use std::collections::{HashMap, HashSet};

use emela_resolve::hir::{
    Arg, BinaryOp, DefId, DefKind, DefModule, Expr, ExprId, ExprKind, Fields, Item, Label, Literal,
    Program, Res, Span,
};
use emela_types::ProgramTypes;

#[derive(Clone, Copy)]
enum Value {
    Int(&'static str),
    Ref(DefId),
    Config(DefId),
    EmptyList,
}

pub(crate) struct Defaults {
    signatures: HashMap<DefId, Vec<(String, Option<Value>)>>,
}

impl Defaults {
    pub(crate) fn new(program: &Program) -> Self {
        let mut defaults = Self {
            signatures: HashMap::new(),
        };
        let gpio = "Embedded.Gpio";
        let uart = "Embedded.Uart";
        let i2c = "Embedded.I2c";
        let spi = "Embedded.Spi";
        let reference = |module, name| Value::Ref(definition(program, module, name, DefKind::Ctor));
        let config = |module, name| Value::Config(definition(program, module, name, DefKind::Ctor));
        let normal = reference("Embedded.Interrupt", "Normal");
        let timeout = Value::Int("1000");

        for (name, initial) in [
            ("Input", None),
            ("OutputPushPull", Some("Low")),
            ("OutputOpenDrain", Some("High")),
            ("Analog", None),
        ] {
            let mut values = vec![("pull", reference(gpio, "NoPull"))];
            if let Some(initial) = initial {
                values.push(("initial", reference(gpio, initial)));
            }
            defaults.register(program, gpio, name, DefKind::Ctor, &values);
        }
        defaults.register(
            program,
            uart,
            "UartConfig",
            DefKind::Ctor,
            &[
                ("data_bits", reference(uart, "Bits8")),
                ("stop_bits", reference(uart, "One")),
                ("parity", reference(uart, "NoParity")),
                ("direction", reference(uart, "Both")),
                ("flow_control", reference(uart, "NoFlowControl")),
                ("oversampling", reference(uart, "Native")),
            ],
        );
        defaults.register(
            program,
            i2c,
            "I2cConfig",
            DefKind::Ctor,
            &[
                ("clock_speed_hz", Value::Int("100000")),
                ("duty_cycle", reference(i2c, "Standard")),
            ],
        );
        defaults.register(
            program,
            spi,
            "SpiConfig",
            DefKind::Ctor,
            &[
                ("mode", reference(spi, "Master")),
                ("direction", reference(spi, "TwoLine")),
                ("data_size", reference(spi, "Bits8")),
                ("polarity", reference(spi, "IdleLow")),
                ("phase", reference(spi, "FirstEdge")),
                ("max_frequency_hz", Value::Int("4000000")),
                ("bit_order", reference(spi, "MostSignificantFirst")),
            ],
        );
        defaults.register(
            program,
            spi,
            "SpiDeviceConfig",
            DefKind::Ctor,
            &[
                ("config", config(spi, "SpiConfig")),
                ("active", reference(gpio, "Low")),
                ("setup_us", Value::Int("0")),
                ("hold_us", Value::Int("0")),
            ],
        );

        for (module, operations) in [
            (uart, &["transmit", "receive", "receive_some"][..]),
            (
                i2c,
                &[
                    "master_transmit",
                    "write_read",
                    "memory_write",
                    "is_device_ready",
                    "recover",
                ][..],
            ),
            (spi, &["transmit", "receive", "transfer", "transaction"][..]),
        ] {
            for name in operations {
                let mut values = vec![("timeout_ms", timeout)];
                match (module, *name) {
                    ("Embedded.I2c", "write_read") => values.push(("write_data", Value::EmptyList)),
                    ("Embedded.I2c", "memory_write") => {
                        values.push(("address_size", reference(i2c, "Bits8")))
                    }
                    ("Embedded.I2c", "is_device_ready") => values.push(("trials", Value::Int("1"))),
                    _ => {}
                }
                defaults.register(program, module, name, DefKind::Op, &values);
            }
        }
        for (module, values) in [
            (gpio, vec![("priority", normal)]),
            (
                uart,
                vec![
                    ("priority", normal),
                    ("notify_transmit", reference("Prelude", "False")),
                ],
            ),
            (i2c, vec![("priority", normal)]),
        ] {
            defaults.register(program, module, "enable_interrupt", DefKind::Op, &values);
        }
        for (module, value) in [
            (i2c, config(i2c, "I2cConfig")),
            (spi, config(spi, "SpiConfig")),
        ] {
            defaults.register(
                program,
                module,
                "configure",
                DefKind::Op,
                &[("config", value)],
            );
        }
        defaults
    }

    fn register(
        &mut self,
        program: &Program,
        module: &str,
        name: &str,
        kind: DefKind,
        values: &[(&str, Value)],
    ) {
        let def = definition(program, module, name, kind);
        let names: Vec<_> = match &program.items[def] {
            Item::Ctor(ctor) => match &ctor.fields {
                Fields::Named(fields) => {
                    fields.iter().map(|field| field.name.to_string()).collect()
                }
                _ => panic!("bundled default constructor must have named fields"),
            },
            Item::Op(op) => op
                .sig
                .params
                .iter()
                .map(|param| program.locals[param.local].name.to_string())
                .collect(),
            _ => unreachable!("defaults only apply to constructors and operations"),
        };
        assert!(
            values
                .iter()
                .all(|(name, _)| names.iter().any(|label| label == name)),
            "default parameter missing from {module}.{name}"
        );
        self.signatures.insert(
            def,
            names
                .into_iter()
                .map(|name| {
                    let value = values
                        .iter()
                        .find(|(label, _)| *label == name)
                        .map(|(_, value)| *value);
                    (name, value)
                })
                .collect(),
        );
    }

    /// Before checking, fills direct constructor calls. After checking, also
    /// fills operations identified by upstream, including aliased capabilities.
    /// Returns whether a fresh type check is necessary.
    pub(crate) fn complete(&self, program: &mut Program, types: Option<&ProgramTypes>) -> bool {
        let piped: HashSet<_> = program
            .exprs
            .iter()
            .filter_map(|(_, expr)| match expr.kind {
                ExprKind::Binary {
                    op: BinaryOp::Pipe,
                    rhs,
                    ..
                } => Some(rhs),
                _ => None,
            })
            .collect();
        let sites: Vec<_> = program
            .exprs
            .iter()
            .filter_map(|(id, expr)| {
                let (callee, args, pipe) = match &expr.kind {
                    ExprKind::Call { callee, args } if !piped.contains(&id) => {
                        (*callee, args.clone(), None)
                    }
                    ExprKind::Binary {
                        op: BinaryOp::Pipe,
                        lhs,
                        rhs,
                    } => {
                        let (callee, args) = match &program.exprs[*rhs].kind {
                            ExprKind::Call { callee, args } => (*callee, args.clone()),
                            _ => (*rhs, Vec::new()),
                        };
                        let mut combined = vec![Arg {
                            label: None,
                            value: *lhs,
                            punned: false,
                        }];
                        combined.extend(args);
                        (callee, combined, Some(*rhs))
                    }
                    _ => return None,
                };
                let def = match program.exprs[callee].kind {
                    ExprKind::Path(Res::Def(def)) => def,
                    _ => *types?.ops.get(callee)?,
                };
                let signature = self.signatures.get(&def)?;
                let missing = missing_defaults(signature, &args)?;
                // Only additions are retained, in declaration order. Written
                // arguments keep their original order and are evaluated once.
                Some((id, pipe, missing, expr.span))
            })
            .collect();

        for (id, pipe, missing, span) in &sites {
            let additions: Vec<_> = missing
                .iter()
                .map(|(name, value)| Arg {
                    label: Some(Label {
                        name: name.as_str().into(),
                        span: *span,
                    }),
                    value: self.expression(program, *value, *span),
                    punned: false,
                })
                .collect();
            let target = if let Some(rhs) = pipe {
                if !matches!(program.exprs[*rhs].kind, ExprKind::Call { .. }) {
                    let callee = program.exprs.alloc(program.exprs[*rhs].clone());
                    program.exprs[*rhs].kind = ExprKind::Call {
                        callee,
                        args: Vec::new(),
                    };
                }
                *rhs
            } else {
                *id
            };
            if let ExprKind::Call { args, .. } = &mut program.exprs[target].kind {
                args.extend(additions);
            }
        }
        !sites.is_empty()
    }

    fn expression(&self, program: &mut Program, value: Value, span: Span) -> ExprId {
        let kind = match value {
            Value::Int(value) => ExprKind::Lit(Literal::Int(value.into())),
            Value::Ref(def) => ExprKind::Path(Res::Def(def)),
            Value::EmptyList => ExprKind::List {
                elems: Vec::new(),
                rest: None,
            },
            Value::Config(def) => {
                let callee = program.exprs.alloc(Expr {
                    kind: ExprKind::Path(Res::Def(def)),
                    span,
                });
                let args = self.signatures[&def]
                    .iter()
                    .map(|(name, value)| Arg {
                        label: Some(Label {
                            name: name.as_str().into(),
                            span,
                        }),
                        value: self.expression(
                            program,
                            value.expect("default config must be complete"),
                            span,
                        ),
                        punned: false,
                    })
                    .collect();
                ExprKind::Call { callee, args }
            }
        };
        program.exprs.alloc(Expr { kind, span })
    }
}

/// Invalid labels, duplicates, excess arguments and missing required arguments
/// are left untouched so upstream reports its ordinary diagnostics.
fn missing_defaults(
    signature: &[(String, Option<Value>)],
    args: &[Arg],
) -> Option<Vec<(String, Value)>> {
    if args.len() >= signature.len() {
        return None;
    }
    let mut supplied = vec![false; signature.len()];
    for (position, arg) in args.iter().enumerate() {
        let slot = match &arg.label {
            Some(label) => signature.iter().position(|(name, _)| *name == label.name)?,
            None => position,
        };
        if std::mem::replace(&mut supplied[slot], true) {
            return None;
        }
    }
    signature
        .iter()
        .zip(supplied)
        .filter(|(_, supplied)| !supplied)
        .map(|((name, value), _)| Some((name.clone(), (*value)?)))
        .collect()
}

fn definition(program: &Program, module: &str, name: &str, kind: DefKind) -> DefId {
    if module == "Prelude" {
        return program.prelude_def(name).expect("missing prelude default");
    }
    program
        .defs
        .iter()
        .find(|(_, def)| {
            def.kind == kind
                && def.name == name
                && match def.module {
                    DefModule::Source(id) => program.modules[id].name.to_string() == module,
                    DefModule::Prelude => module == "Prelude",
                    DefModule::Builtin(_) => false,
                }
        })
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("missing bundled default definition {module}.{name}"))
}
