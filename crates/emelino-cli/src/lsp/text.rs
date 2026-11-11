//! LSP uses UTF-16 columns; Emela spans use UTF-8 byte offsets.
use lsp_types::{Position, Range, Uri};
use rowan::{TextRange, TextSize};
use std::path::{Component, Path, PathBuf};

pub fn uri(path: &Path) -> Result<Uri, String> {
    let path = path.to_str().ok_or("LSP paths must be UTF-8")?;
    let mut encoded = String::from("file://");
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~:".contains(&byte) {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
        .parse()
        .map_err(|e| format!("invalid file URI: {e}"))
}

pub fn path(uri: &Uri) -> Option<PathBuf> {
    let value = uri.as_str().strip_prefix("file://")?;
    let value = if value.starts_with("localhost/") {
        &value["localhost".len()..]
    } else {
        value
    };
    if !value.starts_with('/') || value.contains(['?', '#']) {
        return None;
    }
    let mut bytes = Vec::new();
    let mut input = value.bytes();
    while let Some(byte) = input.next() {
        if byte == b'%' {
            let a = (input.next()? as char).to_digit(16)?;
            let b = (input.next()? as char).to_digit(16)?;
            bytes.push((a * 16 + b) as u8);
        } else {
            bytes.push(byte);
        }
    }
    let text = String::from_utf8(bytes).ok()?;
    (!text.contains('\0')).then(|| normalize(Path::new(&text)))
}

pub fn normalize(path: &Path) -> PathBuf {
    if let Ok(path) = path.canonicalize() {
        return path;
    }
    let mut result = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => (),
            Component::ParentDir => {
                result.pop();
            }
            part => result.push(part),
        }
    }
    result
}

pub fn offset(text: &str, position: Position) -> Option<TextSize> {
    let mut start = 0;
    for _ in 0..position.line {
        start += text.get(start..)?.find('\n')? + 1;
    }
    let line = text
        .get(start..)?
        .split('\n')
        .next()?
        .trim_end_matches('\r');
    let mut units = 0;
    for (index, ch) in line.char_indices() {
        if units == position.character {
            return u32::try_from(start + index).ok().map(Into::into);
        }
        units += ch.len_utf16() as u32;
        if units > position.character {
            return None;
        }
    }
    (units == position.character)
        .then_some(start + line.len())
        .and_then(|n| u32::try_from(n).ok())
        .map(Into::into)
}

pub fn position(text: &str, offset: TextSize) -> Position {
    let mut end = usize::from(offset).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let before = &text[..end];
    let start = before.rfind('\n').map_or(0, |i| i + 1);
    Position::new(
        before.bytes().filter(|b| *b == b'\n').count() as u32,
        before[start..].encode_utf16().count() as u32,
    )
}

pub fn range(text: &str, range: TextRange) -> Range {
    Range::new(position(text, range.start()), position(text, range.end()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_positions_are_byte_boundary_safe() {
        let source = "a😀日本\r\nx";
        assert_eq!(offset(source, Position::new(0, 1)), Some(1.into()));
        assert_eq!(offset(source, Position::new(0, 2)), None);
        assert_eq!(offset(source, Position::new(0, 3)), Some(5.into()));
        assert_eq!(position(source, 5.into()), Position::new(0, 3));
        assert_eq!(
            offset(source, Position::new(1, 1)),
            Some((source.len() as u32).into())
        );
        assert_eq!(offset(source, Position::new(0, 99)), None);
    }

    #[test]
    fn file_uris_roundtrip_reserved_characters_and_unicode() {
        let original = Path::new("/tmp/emelino-uri 日本語 ' # %?.emel");
        let encoded = uri(original).unwrap();
        assert!(encoded.as_str().contains("%23%20%25%3F"));
        assert_eq!(path(&encoded).as_deref(), Some(original));
        assert_eq!(
            path(&"file://localhost/tmp/emelino-uri.emel".parse().unwrap()),
            Some(PathBuf::from("/tmp/emelino-uri.emel"))
        );
        assert_eq!(
            path(&"https://example.invalid/main.emel".parse().unwrap()),
            None
        );
        assert_eq!(path(&"file://remote/tmp/main.emel".parse().unwrap()), None);
        assert_eq!(path(&"file:///tmp/%00.emel".parse().unwrap()), None);
    }
}
