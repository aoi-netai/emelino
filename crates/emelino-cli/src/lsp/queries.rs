//! Editor queries over Emela's public HIR and type tables. No type inference is duplicated.
use emela_resolve::hir::{
    DefId, DefKind, DefModule, Expr, ExprId, ExprKind, Fields, Item, LocalId, LocalKind, PatId,
    PatKind, Program, RefTarget, Res, Span, Stmt,
};
use emela_resolve::{ModuleId, ModuleName};
use emela_syntax::ast::AstNode;
use emela_types::{Ty, TyParam};
use emelino_cli::analysis::Analysis;
use lsp_types::{CompletionItem, CompletionItemKind, Documentation, MarkupContent, MarkupKind};
use rowan::{TextRange, TextSize};
use std::collections::HashSet;

#[derive(Clone, Copy)]
pub enum Symbol {
    Def(DefId),
    Local(LocalId),
    Module(ModuleId),
}

pub fn symbol_at(a: &Analysis, module: ModuleId, offset: TextSize) -> Option<(Symbol, TextRange)> {
    let mut found = Vec::new();
    let mut add = |symbol, span: Span| {
        if span.module == module && span.range.contains_inclusive(offset) {
            found.push((symbol, span.range));
        }
    };
    for reference in &a.hir.refs {
        add(
            match reference.target {
                RefTarget::Def(d) => Symbol::Def(d),
                RefTarget::Local(l) => Symbol::Local(l),
                RefTarget::Module(m) => Symbol::Module(m),
            },
            reference.span,
        );
    }
    for (expr, &def) in a.types.ops.iter().chain(a.types.methods.iter()) {
        if let ExprKind::Field { name, .. } = &a.hir.exprs[expr].kind {
            add(Symbol::Def(def), name.span);
        }
    }
    for (def, data) in a.hir.defs.iter() {
        if data.kind != DefKind::Impl
            && let Some(span) = data.span
        {
            add(Symbol::Def(def), span);
        }
    }
    for (local, data) in a.hir.locals.iter() {
        add(Symbol::Local(local), data.span);
    }
    found.sort_by_key(|(_, range)| (!range.contains(offset), range.len()));
    found.into_iter().next()
}

pub fn definition(a: &Analysis, module: ModuleId, offset: TextSize) -> Option<Span> {
    let span = if let Some((symbol, _)) = symbol_at(a, module, offset) {
        match symbol {
            Symbol::Def(def) => a.hir.defs[def].span,
            Symbol::Local(local) => Some(a.hir.locals[local].span),
            Symbol::Module(module) => Some(Span {
                module,
                range: TextRange::empty(0.into()),
            }),
        }
    } else {
        a.hir.exprs.iter().find_map(|(_, expr)| {
            let ExprKind::Field { base, name } = &expr.kind else {
                return None;
            };
            if name.span.module != module || !name.span.range.contains_inclusive(offset) {
                return None;
            }
            let Ty::Named(con, _) = expr_type(a, *base)? else {
                return None;
            };
            let fields = match a.hir.item(a.types.ty_table.def(con))? {
                Item::Type(t) => match &t.fields {
                    Fields::Named(fields) => &fields[..],
                    _ => return None,
                },
                Item::Error(e) => match &e.fields {
                    Fields::Named(fields) => &fields[..],
                    _ => return None,
                },
                Item::Handler(h) => &h.fields,
                _ => return None,
            };
            fields
                .iter()
                .find(|field| field.name == name.name)
                .map(|field| field.span)
        })
    };
    span.filter(|span| !a.modules.is_core(span.module))
}

fn params(a: &Analysis, module: ModuleId, offset: TextSize) -> Vec<TyParam> {
    a.hir
        .items
        .iter()
        .filter_map(|(def, item)| {
            let body = match item {
                Item::Fn(f) | Item::TraitFn(f) => f.body?,
                _ => return None,
            };
            let span = a.hir.exprs[body].span;
            (span.module == module && span.range.contains_inclusive(offset)).then_some(def)
        })
        .find_map(|def| a.types.sigs.get(def).map(|s| s.scheme.params.clone()))
        .unwrap_or_default()
}

pub fn docs(a: &Analysis, def: DefId) -> Option<String> {
    let span = a.hir.defs[def].span?;
    let text = a.texts.get(span.module)?;
    let before = text.get(..usize::from(span.range.start()))?;
    let start = before.rfind('\n').map_or(0, |i| i + 1);
    let mut lines = Vec::new();
    for line in before[..start].lines().rev() {
        if let Some(line) = line.trim_start().strip_prefix("##") {
            lines.push(line.trim_start());
        } else {
            break;
        }
    }
    lines.reverse();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

pub fn detail(a: &Analysis, def: DefId) -> String {
    let data = &a.hir.defs[def];
    if let Some(sig) = a.types.sigs.get(def) {
        format!(
            "{}: {}",
            data.name,
            sig.scheme.display(&a.types.ty_table.cons)
        )
    } else {
        format!("{} {}", data.kind.describe(), data.name)
    }
}

pub fn hover(a: &Analysis, module: ModuleId, offset: TextSize) -> Option<(TextRange, String)> {
    let cons = &a.types.ty_table.cons;
    if let Some((symbol, range)) = symbol_at(a, module, offset) {
        let (value, doc) = match symbol {
            Symbol::Def(def) => (detail(a, def), docs(a, def)),
            Symbol::Local(local) => {
                let ty = a.types.locals.get(local)?;
                (
                    format!(
                        "{}: {}",
                        a.hir.locals[local].name,
                        ty.display_with(cons, &params(a, module, offset))
                    ),
                    None,
                )
            }
            Symbol::Module(module) => (format!("module {}", a.modules[module].name), None),
        };
        let mut markdown = format!("```emela\n{value}\n```");
        if let Some(doc) = doc {
            markdown.push_str(&format!("\n\n{doc}"));
        }
        return Some((range, markdown));
    }
    a.hir
        .exprs
        .iter()
        .filter(|(_, e)| e.span.module == module && e.span.range.contains(offset))
        .filter_map(|(id, e)| a.types.exprs.get(id).map(|ty| (e.span.range, ty)))
        .min_by_key(|(range, _)| range.len())
        .map(|(range, ty)| {
            (
                range,
                format!(
                    "```emela\n{}\n```",
                    ty.display_with(cons, &params(a, module, offset))
                ),
            )
        })
}

fn kind(kind: DefKind) -> CompletionItemKind {
    match kind {
        DefKind::Fn | DefKind::ImplFn => CompletionItemKind::FUNCTION,
        DefKind::Op | DefKind::TraitFn => CompletionItemKind::METHOD,
        DefKind::Ctor => CompletionItemKind::CONSTRUCTOR,
        DefKind::Const => CompletionItemKind::CONSTANT,
        DefKind::Enum => CompletionItemKind::ENUM,
        DefKind::Trait => CompletionItemKind::INTERFACE,
        _ => CompletionItemKind::CLASS,
    }
}

fn def_item(a: &Analysis, def: DefId) -> CompletionItem {
    CompletionItem {
        label: a.hir.defs[def].name.to_string(),
        kind: Some(kind(a.hir.defs[def].kind)),
        detail: Some(detail(a, def)),
        documentation: docs(a, def).map(|value| {
            Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value,
            })
        }),
        ..CompletionItem::default()
    }
}

fn module_item(label: String, detail: String) -> CompletionItem {
    CompletionItem {
        label,
        kind: Some(CompletionItemKind::MODULE),
        detail: Some(detail),
        ..CompletionItem::default()
    }
}

fn members(a: &Analysis, module: ModuleId) -> Vec<CompletionItem> {
    a.hir
        .defs
        .iter()
        .filter(|(_, d)| {
            d.module == DefModule::Source(module)
                && d.is_pub
                && (d.parent.is_none() || d.kind == DefKind::Ctor)
                && !matches!(d.kind, DefKind::TypeParam | DefKind::ImplFn | DefKind::Impl)
        })
        .map(|(id, _)| def_item(a, id))
        .collect()
}

fn imported_module(a: &Analysis, module: ModuleId, alias: &str) -> Option<ModuleId> {
    let tree = a.trees.get(module)?;
    for import in tree.imports() {
        let path = import.path()?;
        let parts: Vec<String> = path.segments().map(|t| t.text().to_string()).collect();
        if parts.last().is_some_and(|part| part == alias) {
            return a.modules.lookup(&ModuleName::new(parts));
        }
    }
    None
}

fn expr_type(a: &Analysis, expr: ExprId) -> Option<Ty> {
    if let Some(ty) = a.types.exprs.get(expr) {
        return Some(ty.clone());
    }
    match &a.hir.exprs[expr].kind {
        ExprKind::Path(Res::Local(local)) => a.types.locals.get(*local).cloned(),
        ExprKind::Path(Res::Def(def)) => a.types.sigs.get(*def).map(|s| s.scheme.ty.clone()),
        ExprKind::Call { callee, .. } => match expr_type(a, *callee)? {
            Ty::Fn(f) => Some(f.ret.clone()),
            _ => None,
        },
        _ => None,
    }
}

pub fn complete(a: &Analysis, module: ModuleId, offset: TextSize) -> Vec<CompletionItem> {
    let Some(text) = a.texts.get(module) else {
        return Vec::new();
    };
    let Some(before) = text.get(..usize::from(offset)) else {
        return Vec::new();
    };
    let is_name = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let start = before.trim_end_matches(is_name).len();
    let context = &before[..start];
    let line = &before[before[..start].rfind('\n').map_or(0, |i| i + 1)..start];
    let mut items = if let Some(rest) = line.trim_start().strip_prefix("import ") {
        if let Some((path, _)) = rest.split_once('{') {
            a.modules
                .lookup(&ModuleName::parse(path.trim().trim_end_matches('.')))
                .map(|m| members(a, m))
                .unwrap_or_default()
        } else {
            let prefix: Vec<_> = rest.trim().split('.').collect();
            let prefix = &prefix[..prefix.len().saturating_sub(1)];
            a.modules
                .iter()
                .filter_map(|(_, data)| {
                    let segments = data.name.segments();
                    (segments.len() > prefix.len()
                        && segments.iter().zip(prefix).all(|(s, p)| s == p))
                    .then(|| module_item(segments[prefix.len()].to_string(), data.name.to_string()))
                })
                .collect()
        }
    } else if let Some(left) = context.strip_suffix('.') {
        let name = &left[left
            .trim_end_matches(|c: char| is_name(c) || c == '.')
            .len()..];
        let target = imported_module(a, module, name).or_else(|| {
            a.modules
                .lookup(&ModuleName::parse(name))
                .filter(|&m| a.modules.is_core(m))
        });
        if let Some(target) = target {
            members(a, target)
        } else {
            let mut result = Vec::new();
            let expr = a
                .hir
                .exprs
                .iter()
                .filter(|(_, e)| {
                    e.span.module == module && usize::from(e.span.range.end()) == left.len()
                })
                .filter(|(_, e)| !matches!(e.kind, ExprKind::Missing))
                .max_by_key(|(_, e)| e.span.range.len())
                .map(|(id, _)| id);
            let ty = expr.and_then(|expr| expr_type(a, expr)).or_else(|| {
                visible_locals(&a.hir, module, offset)
                    .into_iter()
                    .find(|&l| a.hir.locals[l].name == name)
                    .and_then(|l| a.types.locals.get(l).cloned())
            });
            if let Some(ty) = ty {
                match &ty {
                    Ty::Cap(tag) => result.extend(
                        a.hir
                            .effect_ops(tag.def)
                            .iter()
                            .map(|&def| def_item(a, def)),
                    ),
                    Ty::Named(con, _) => {
                        let fields = match a.hir.item(a.types.ty_table.def(*con)) {
                            Some(Item::Type(t)) => match &t.fields {
                                Fields::Named(f) => &f[..],
                                _ => &[],
                            },
                            Some(Item::Error(e)) => match &e.fields {
                                Fields::Named(f) => &f[..],
                                _ => &[],
                            },
                            Some(Item::Handler(h)) => &h.fields,
                            _ => &[],
                        };
                        result.extend(fields.iter().map(|field| CompletionItem {
                            label: field.name.to_string(),
                            kind: Some(CompletionItemKind::FIELD),
                            ..CompletionItem::default()
                        }));
                    }
                    _ => (),
                }
                result.extend(
                    emela_types::methods_of(
                        &a.hir,
                        &a.types,
                        module,
                        &ty,
                        &params(a, module, offset),
                    )
                    .into_iter()
                    .map(|def| def_item(a, def)),
                );
            }
            result
        }
    } else {
        let mut result = Vec::new();
        for local in visible_locals(&a.hir, module, offset) {
            result.push(CompletionItem {
                label: a.hir.locals[local].name.to_string(),
                kind: Some(CompletionItemKind::VARIABLE),
                detail: a.types.locals.get(local).map(|ty| {
                    ty.display_with(&a.types.ty_table.cons, &params(a, module, offset))
                        .to_string()
                }),
                ..CompletionItem::default()
            });
        }
        let imports: Vec<_> = a
            .trees
            .get(module)
            .into_iter()
            .flat_map(|tree| tree.imports())
            .collect();
        let imported: HashSet<_> = a
            .hir
            .refs
            .iter()
            .filter(|r| {
                r.span.module == module
                    && imports
                        .iter()
                        .any(|i| i.syntax().text_range().contains_range(r.span.range))
            })
            .filter_map(|r| match r.target {
                RefTarget::Def(d) => Some(d),
                _ => None,
            })
            .collect();
        result.extend(
            a.hir
                .defs
                .iter()
                .filter(|(id, d)| {
                    imported.contains(id)
                        || (d.module == DefModule::Source(module)
                            && (d.parent.is_none() || d.kind == DefKind::Ctor)
                            && d.kind != DefKind::Impl)
                        || a.hir.prelude.defs.contains(id)
                })
                .map(|(def, _)| def_item(a, def)),
        );
        for import in imports {
            if let Some(path) = import.path() {
                let parts: Vec<_> = path.segments().map(|t| t.text().to_string()).collect();
                if let Some(last) = parts.last() {
                    result.push(module_item(last.clone(), parts.join(".")));
                }
            }
        }
        result.extend(
            a.modules
                .iter()
                .filter(|(id, data)| a.modules.is_core(*id) && data.name.segments().len() == 1)
                .map(|(_, data)| module_item(data.name.to_string(), data.name.to_string())),
        );
        result.extend(
            [
                "fn", "pub", "type", "enum", "error", "effect", "handler", "layer", "trait",
                "impl", "import", "if", "else", "match", "fail", "escape", "use", "with", "assert",
                "suspend", "const", "opaque",
            ]
            .into_iter()
            .map(|word| CompletionItem {
                label: word.into(),
                kind: Some(CompletionItemKind::KEYWORD),
                ..CompletionItem::default()
            }),
        );
        result
    };
    let mut seen = HashSet::new();
    items.retain(|item| seen.insert(item.label.clone()));
    items
}

// Lexical scope collection adapted from Emela v0.27.0,
// emela-driver/src/complete.rs (Apache-2.0).
fn visible_locals(
    hir: &Program,
    module: emela_resolve::ModuleId,
    offset: TextSize,
) -> Vec<LocalId> {
    let mut scopes: Vec<(LocalId, TextRange)> = Vec::new();
    let mut add = |locals: Vec<LocalId>, range: TextRange| {
        scopes.extend(locals.into_iter().map(|l| (l, range)));
    };
    let span_of = |expr: emela_resolve::hir::ExprId| hir.exprs[expr].span.range;
    for (_, item) in hir.items.iter() {
        match item {
            Item::Fn(f) | Item::TraitFn(f) => {
                if let Some(body) = f.body {
                    add(
                        f.sig.params.iter().map(|p| p.local).collect(),
                        span_of(body),
                    );
                }
            }
            Item::Handler(h) => {
                for op in &h.ops {
                    if let Some(body) = op.body {
                        let mut locals = vec![op.self_local];
                        locals.extend(op.params.iter().map(|p| p.local));
                        add(locals, span_of(body));
                    }
                }
                if let Some(body) = h.init {
                    add(
                        h.init_params.iter().map(|p| p.local).collect(),
                        span_of(body),
                    );
                }
                if let Some((local, body)) = h.release {
                    add(vec![local], span_of(body));
                }
            }
            Item::Impl(i) => {
                for f in &i.fns {
                    if let Some(body) = f.body {
                        add(f.params.iter().map(|p| p.local).collect(), span_of(body));
                    }
                }
            }
            _ => {}
        }
    }
    for (_, Expr { kind, span }) in hir.exprs.iter() {
        if span.module != module || !span.range.contains_inclusive(offset) {
            continue;
        }
        match kind {
            ExprKind::Block(block) => {
                for stmt in &block.stmts {
                    if let Stmt::Let { pat, value, .. } = stmt {
                        // From after the right-hand side to the end of the block (spec 6.3).
                        let start = span_of(*value).end();
                        if start <= span.range.end() {
                            add(
                                pat_locals(hir, *pat),
                                TextRange::new(start, span.range.end()),
                            );
                        }
                    }
                }
            }
            ExprKind::Lambda { params, body } => {
                // A parameter in place of a pattern has no name in scope; the names of the
                // pattern come from the binding at the head of the body.
                add(
                    params
                        .iter()
                        .map(|p| p.local)
                        .filter(|&l| hir.locals[l].kind != LocalKind::PatParam)
                        .collect(),
                    span_of(*body),
                );
            }
            ExprKind::Match { arms, .. } | ExprKind::Escape { arms, .. } => {
                for arm in arms {
                    let range = hir.pats[arm.pat].span.range.cover(span_of(arm.body));
                    add(pat_locals(hir, arm.pat), range);
                }
            }
            _ => {}
        }
    }
    let mut visible: Vec<LocalId> = scopes
        .into_iter()
        .filter(|&(local, range)| {
            hir.locals[local].span.module == module && range.contains_inclusive(offset)
        })
        .map(|(local, _)| local)
        .collect();
    // Innermost (bound later) first.
    visible.sort_by_key(|&l| std::cmp::Reverse(hir.locals[l].span.range.start()));
    let mut seen = std::collections::HashSet::new();
    visible.retain(|&l| seen.insert(hir.locals[l].name.clone()));
    visible
}

fn pat_locals(hir: &Program, pat: PatId) -> Vec<LocalId> {
    let mut locals = Vec::new();
    let mut stack = vec![pat];
    while let Some(pat) = stack.pop() {
        match &hir.pats[pat].kind {
            PatKind::Bind(local) => locals.push(*local),
            PatKind::Ctor {
                args: Some(args), ..
            } => stack.extend(args.iter().map(|a| a.pat)),
            PatKind::Tuple(pats) => stack.extend(pats.iter().copied()),
            PatKind::List { elems, rest } => {
                stack.extend(elems.iter().copied());
                if let Some(binding) = rest.and_then(|r| r.binding) {
                    locals.push(binding);
                }
            }
            _ => {}
        }
    }
    locals
}
