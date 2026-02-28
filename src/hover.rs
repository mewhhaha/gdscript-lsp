use crate::parser::{ParsedScript, ScriptDeclKind};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hover {
    pub title: String,
    pub body: String,
}

pub fn hover_at(line: usize, character: usize, script: &ParsedScript) -> Option<Hover> {
    let line_text = script
        .lines
        .get(line.saturating_sub(1))
        .map(String::as_str)?;

    if let Some(symbol) = identifier_at(line_text, character) {
        if let Some(hover) = builtin_hover(&symbol) {
            return Some(hover);
        }

        if let Some(decl) = script.declarations.iter().find(|decl| decl.name == symbol) {
            return Some(Hover {
                title: format!("{} '{}'", decl.kind.kind_label(), decl.name),
                body: format!(
                    "{} declaration in {}",
                    decl.kind.kind_label(),
                    script.path.display()
                ),
            });
        }
    }

    script
        .declarations
        .iter()
        .find(|decl| decl.line == line)
        .map(|decl| Hover {
            title: format!("{} '{}'", decl.kind.kind_label(), decl.name),
            body: format!(
                "{} declaration in {}",
                decl.kind.kind_label(),
                script.path.display()
            ),
        })
}

fn builtin_hover(name: &str) -> Option<Hover> {
    if let Some((signature, body)) = builtin_hover_metadata().get(name) {
        return Some(Hover {
            title: format!("builtin {signature}"),
            body: body.clone(),
        });
    }

    match name {
        "print" => Some(Hover {
            title: "builtin print(...)".to_string(),
            body: "GDScript builtin: prints values to output".to_string(),
        }),
        "preload" => Some(Hover {
            title: "builtin preload(path)".to_string(),
            body: "GDScript builtin: loads a resource at parse time".to_string(),
        }),
        "len" => Some(Hover {
            title: "builtin len(value)".to_string(),
            body: "GDScript builtin: returns collection length".to_string(),
        }),
        _ => None,
    }
}

fn builtin_hover_metadata() -> &'static HashMap<String, (String, String)> {
    static BUILTIN_META: OnceLock<HashMap<String, (String, String)>> = OnceLock::new();
    BUILTIN_META.get_or_init(|| {
        include_str!("../data/godot_4_6_builtin_meta.tsv")
            .lines()
            .skip(1)
            .filter_map(|line| {
                let mut fields = line.splitn(3, '\t');
                let name = fields.next()?.trim();
                let signature = fields.next()?.trim();
                let hover = fields.next()?.trim();
                if name.is_empty() || signature.is_empty() || hover.is_empty() {
                    return None;
                }
                Some((name.to_string(), (signature.to_string(), hover.to_string())))
            })
            .collect()
    })
}

fn identifier_at(line: &str, character: usize) -> Option<String> {
    if line.is_empty() {
        return None;
    }

    let mut byte_index = character.saturating_sub(1);
    if byte_index >= line.len() {
        byte_index = line.len().saturating_sub(1);
    }

    let bytes = line.as_bytes();
    while byte_index > 0 && !is_ident_char(bytes[byte_index]) {
        byte_index -= 1;
    }

    if !is_ident_char(bytes[byte_index]) {
        return None;
    }

    let mut start = byte_index;
    let mut end = byte_index;

    while start > 0 && is_ident_char(bytes[start - 1]) {
        start -= 1;
    }
    while end + 1 < bytes.len() && is_ident_char(bytes[end + 1]) {
        end += 1;
    }

    Some(line[start..=end].to_string())
}

fn is_ident_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

impl ScriptDeclKind {
    fn kind_label(&self) -> &'static str {
        match self {
            ScriptDeclKind::Function => "function",
            ScriptDeclKind::Class => "class",
            ScriptDeclKind::Variable => "variable",
            ScriptDeclKind::Constant => "constant",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::identifier_at;

    #[test]
    fn extracts_identifier_at_cursor() {
        assert_eq!(
            identifier_at("    print(\"x\")", 7).as_deref(),
            Some("print")
        );
    }
}
