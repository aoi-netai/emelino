//! Test-only migration of frozen v0.26 source; never used by the product frontend.
#![allow(dead_code)]
pub fn syntax(mut text: String) -> String {
    loop {
        let parsed = emela_syntax::parse(&text);
        let mut edits = Vec::new();
        for node in parsed.syntax().descendants() {
            if let Some((range, replacement, _)) = emela_syntax::rewrite::old_lambda(&node) {
                edits.push((
                    usize::from(range.start())..usize::from(range.end()),
                    replacement,
                ));
            }
            if let Some((_, replacement)) = emela_syntax::rewrite::old_uses_clause(&node) {
                let r = node.text_range();
                edits.push((r.into(), replacement));
            }
        }
        if edits.is_empty() {
            break;
        }
        edits.sort_by_key(|(r, _)| (r.len(), r.start));
        let mut selected: Vec<_> = Vec::new();
        for edit in edits {
            if !selected
                .iter()
                .any(|(r, _): &(std::ops::Range<usize>, String)| {
                    r.start < edit.0.end && edit.0.start < r.end
                })
            {
                selected.push(edit);
            }
        }
        selected.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
        for (r, s) in selected {
            text.replace_range(r, &s);
        }
    }
    text
}

pub fn source(text: &str) -> String {
    use emela_syntax::SyntaxKind::{LOWER_NAME, TYPE_NAME, UPPER_NAME};
    use emela_syntax::ast::{AstNode, Import};
    let mut text = syntax(text.to_owned());
    let parsed = emela_syntax::parse(&text);
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    let mut imports = Vec::new();
    for import in parsed.syntax().descendants().filter_map(Import::cast) {
        if import
            .path()
            .is_none_or(|p| p.syntax().text() != "Std.Fiber")
        {
            continue;
        }
        let names: Vec<_> = import.names().map(|t| t.text().to_owned()).collect();
        let (moved, keep): (Vec<_>, Vec<_>) = names
            .into_iter()
            .partition(|n| matches!(n.as_str(), "Exit" | "Success" | "Failure"));
        if moved.is_empty() {
            continue;
        }
        imports.push(format!("import Std.Exit.{{{}}}\n", moved.join(", ")));
        let replacement = if keep.is_empty() {
            String::new()
        } else {
            format!("import Std.Fiber.{{{}}}", keep.join(", "))
        };
        edits.push((import.syntax().text_range().into(), replacement));
    }
    let tokens: Vec<_> = parsed
        .syntax()
        .descendants_with_tokens()
        .filter_map(|n| n.into_token())
        .filter(|t| !t.kind().is_trivia())
        .collect();
    let mut qualified = false;
    for triple in tokens.windows(3) {
        if matches!(triple[0].kind(), TYPE_NAME | UPPER_NAME)
            && triple[0].text() == "Fiber"
            && triple[1].text() == "."
            && matches!(triple[2].kind(), LOWER_NAME | TYPE_NAME | UPPER_NAME)
            && matches!(triple[2].text(), "attempt" | "Exit" | "Success" | "Failure")
        {
            qualified = true;
            edits.push((triple[0].text_range().into(), "Exit".into()));
        }
    }
    if qualified && imports.is_empty() {
        imports.push("import Std.Exit\n".into());
    }
    edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    for (range, replacement) in edits {
        text.replace_range(range, &replacement);
    }
    format!("{}{text}", imports.concat())
}
