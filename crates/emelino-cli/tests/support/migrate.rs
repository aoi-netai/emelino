//! Test-only migration of frozen v0.26 source; never used by the product frontend.
#![allow(dead_code)]
pub fn syntax(mut text: String) -> String {
    loop {
        let parsed = emela_syntax::parse(&text);
        let mut edits = Vec::new();
        for node in parsed.syntax().descendants() {
            if let Some((range, replacement, _)) = emela_syntax::rewrite::old_lambda(&node) {
                edits.push((usize::from(range.start())..usize::from(range.end()), replacement));
            }
            if let Some((_, replacement)) = emela_syntax::rewrite::old_uses_clause(&node) {
                let r = node.text_range();
                edits.push((r.into(), replacement));
            }
        }
        if edits.is_empty() { break }
        edits.sort_by_key(|(r, _)| (r.len(), r.start));
        let mut selected: Vec<_> = Vec::new();
        for edit in edits {
            if !selected.iter().any(|(r, _): &(std::ops::Range<usize>, String)| r.start < edit.0.end && edit.0.start < r.end) {
                selected.push(edit);
            }
        }
        selected.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
        for (r, s) in selected { text.replace_range(r, &s); }
    }
    text
}

pub fn source(text: &str) -> String {
    let mut text = syntax(text.to_owned());
    let mut offset = 0;
    let mut imports = Vec::new();
    while let Some(start) = text[offset..].find("import Std.Fiber.{") {
        let start = offset + start;
        let content = start + "import Std.Fiber.{".len();
        let end = content + text[content..].find('}').expect("frozen import closes");
        let names: Vec<_> = text[content..end].split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
        let (moved, keep): (Vec<_>, Vec<_>) = names.into_iter().partition(|name| matches!(*name, "Exit" | "Success" | "Failure"));
        if !moved.is_empty() {
            imports.push(format!("import Std.Exit.{{{}}}\n", moved.join(", ")));
            let replacement = if keep.is_empty() { String::new() } else { format!("import Std.Fiber.{{{}}}", keep.join(", ")) };
            text.replace_range(start..=end, &replacement);
            offset = start + replacement.len();
        } else { offset = end + 1; }
    }
    let mut qualified = false;
    for name in ["attempt", "Exit", "Success", "Failure"] {
        let old = format!("Fiber.{name}");
        if text.contains(&old) { qualified = true; text = text.replace(&old, &format!("Exit.{name}")); }
    }
    if qualified && imports.is_empty() && !text.contains("import Std.Exit") { imports.push("import Std.Exit\n".into()); }
    format!("{}{text}", imports.concat())
}
