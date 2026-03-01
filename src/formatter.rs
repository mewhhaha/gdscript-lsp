const MAX_CONSECUTIVE_BLANK_LINES: usize = 1;
const FORMAT_LINE_LIMIT: usize = 100;

pub fn format_gdscript(source: &str) -> String {
    let normalized = source.replace('\r', "").replace('\t', "    ");
    let mut out = String::new();
    let has_terminal_newline = normalized.ends_with('\n');
    let lines: Vec<&str> = normalized.trim_end_matches('\n').split('\n').collect();
    let mut blank_run = 0usize;

    for (idx, raw_line) in lines.iter().enumerate() {
        let normalized_line = normalize_line(raw_line);
        if normalized_line.is_empty() {
            blank_run += 1;
            if blank_run > MAX_CONSECUTIVE_BLANK_LINES {
                continue;
            }
        } else {
            blank_run = 0;
        }

        out.push_str(&normalized_line);
        if idx + 1 < lines.len() || has_terminal_newline {
            out.push('\n');
        }
    }

    if normalized.is_empty() {
        out.clear();
    }
    out
}

fn normalize_line(line: &str) -> String {
    let trimmed = line.trim_end();
    if trimmed.is_empty() {
        return String::new();
    }

    let indent_len = trimmed
        .as_bytes()
        .iter()
        .take_while(|byte| byte.is_ascii_whitespace())
        .count();

    if indent_len >= trimmed.len() {
        return String::new();
    }

    let (indent, content) = trimmed.split_at(indent_len);
    let content = normalize_content(content);
    wrap_long_line_if_needed(indent, &content)
}

fn normalize_content(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut out = String::new();
    let mut idx = 0usize;
    let mut quote = None::<u8>;
    let mut escaped = false;
    let mut triple = false;
    let mut square_depth = 0usize;
    let mut curly_depth = 0usize;

    while idx < bytes.len() {
        let ch = bytes[idx];

        if let Some(q) = quote {
            if escaped {
                out.push(ch as char);
                escaped = false;
                idx += 1;
                continue;
            }

            if triple
                && idx + 2 < bytes.len()
                && bytes[idx] == q
                && bytes[idx + 1] == q
                && bytes[idx + 2] == q
            {
                out.push_str(&line[idx..idx + 3]);
                quote = None;
                triple = false;
                idx += 3;
                continue;
            }

            out.push(ch as char);
            if ch == b'\\' && !triple {
                escaped = true;
            } else if ch == q && !triple {
                quote = None;
            }
            idx += 1;
            continue;
        }

        if ch == b'\'' || ch == b'"' {
            quote = Some(ch);
            triple = idx + 2 < bytes.len() && bytes[idx + 1] == ch && bytes[idx + 2] == ch;
            out.push(ch as char);
            idx += 1;
            if triple && idx + 1 < bytes.len() {
                out.push(ch as char);
                out.push(ch as char);
                idx += 2;
            }
            continue;
        }

        if ch == b'#' {
            trim_trailing_spaces(&mut out);
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(&normalize_comment(&line[idx..]));
            break;
        }

        if ch.is_ascii_whitespace() {
            let next_idx = skip_ascii_whitespace(bytes, idx);
            if next_idx >= bytes.len() {
                break;
            }
            if let (Some(prev), Some(next)) = (
                previous_significant_byte(out.as_bytes(), out.len()),
                bytes.get(next_idx).copied(),
            ) && should_emit_space_between(prev, next)
            {
                out.push(' ');
            }
            idx = next_idx;
            continue;
        }

        if ch == b',' {
            trim_trailing_spaces(&mut out);
            out.push(',');
            idx += 1;
            idx = skip_ascii_whitespace(bytes, idx);
            if should_emit_space_after(bytes, idx) {
                out.push(' ');
            }
            continue;
        }

        if let Some(op) = operator_at(bytes, idx) {
            if op == "+" || op == "-" {
                let unary = is_unary_sign(&out);
                if unary {
                    out.push_str(op);
                    idx += op.len();
                    idx = skip_ascii_whitespace(bytes, idx);
                    continue;
                }
            }

            trim_trailing_spaces(&mut out);
            if should_emit_space_before_operator(&out) {
                out.push(' ');
            }
            out.push_str(op);
            idx += op.len();
            idx = skip_ascii_whitespace(bytes, idx);
            if should_emit_space_after(bytes, idx) {
                out.push(' ');
            }
            continue;
        }

        if ch == b':' {
            if square_depth > 0 && curly_depth == 0 {
                out.push(':');
                idx += 1;
                continue;
            }
            trim_trailing_spaces(&mut out);
            out.push(':');
            idx += 1;
            idx = skip_ascii_whitespace(bytes, idx);
            if should_emit_space_after(bytes, idx) {
                out.push(' ');
            }
            continue;
        }

        match ch {
            b'[' => square_depth += 1,
            b']' => square_depth = square_depth.saturating_sub(1),
            b'{' => curly_depth += 1,
            b'}' => curly_depth = curly_depth.saturating_sub(1),
            _ => {}
        }

        out.push(ch as char);
        idx += 1;
    }

    out
}

fn wrap_long_line_if_needed(indent: &str, content: &str) -> String {
    let single_line = format!("{indent}{content}");
    if single_line.len() <= FORMAT_LINE_LIMIT {
        return single_line;
    }

    wrap_multiline_braced_record(indent, content)
        .or_else(|| wrap_multiline_bracket_list(indent, content))
        .or_else(|| wrap_multiline_parenthesized_list(indent, content))
        .unwrap_or(single_line)
}

fn wrap_multiline_braced_record(indent: &str, content: &str) -> Option<String> {
    wrap_multiline_delimited_list(indent, content, b'{', b'}')
}

fn wrap_multiline_parenthesized_list(indent: &str, content: &str) -> Option<String> {
    wrap_multiline_delimited_list(indent, content, b'(', b')')
}

fn wrap_multiline_bracket_list(indent: &str, content: &str) -> Option<String> {
    wrap_multiline_delimited_list(indent, content, b'[', b']')
}

fn wrap_multiline_delimited_list(
    indent: &str,
    content: &str,
    open: u8,
    close: u8,
) -> Option<String> {
    let (open_idx, close_idx) = find_delimited_bounds(content, open, close)?;
    let prefix_raw = &content[..open_idx];
    let prefix = prefix_raw.trim_end();
    let had_space_before_open = prefix_raw.len() > prefix.len();
    let suffix = content[close_idx + 1..].trim();
    let inner = &content[open_idx + 1..close_idx];
    let fields = split_top_level_segments(inner)?;
    if fields.len() < 2 {
        return None;
    }

    let mut out = String::new();
    out.push_str(indent);
    out.push_str(prefix);
    if had_space_before_open || needs_space_before_open_delimiter(prefix, open) {
        out.push(' ');
    }
    out.push(open as char);
    out.push('\n');

    let child_indent = format!("{indent}    ");
    for (idx, field) in fields.iter().enumerate() {
        out.push_str(&child_indent);
        out.push_str(field);
        if idx + 1 < fields.len() {
            out.push(',');
        }
        out.push('\n');
    }

    out.push_str(indent);
    out.push(close as char);
    if !suffix.is_empty() {
        if needs_space_before_suffix(suffix) {
            out.push(' ');
        }
        out.push_str(suffix);
    }

    Some(out)
}

fn find_delimited_bounds(content: &str, open: u8, close: u8) -> Option<(usize, usize)> {
    let bytes = content.as_bytes();
    let mut idx = 0usize;
    let mut quote = None::<u8>;
    let mut escaped = false;
    let mut triple = false;
    let mut open_idx = None::<usize>;
    let mut depth = 0usize;

    while idx < bytes.len() {
        let ch = bytes[idx];
        if let Some(q) = quote {
            if escaped {
                escaped = false;
                idx += 1;
                continue;
            }
            if triple
                && idx + 2 < bytes.len()
                && bytes[idx] == q
                && bytes[idx + 1] == q
                && bytes[idx + 2] == q
            {
                quote = None;
                triple = false;
                idx += 3;
                continue;
            }
            if ch == b'\\' && !triple {
                escaped = true;
            } else if ch == q && !triple {
                quote = None;
            }
            idx += 1;
            continue;
        }

        if ch == b'\'' || ch == b'"' {
            quote = Some(ch);
            triple = idx + 2 < bytes.len() && bytes[idx + 1] == ch && bytes[idx + 2] == ch;
            idx += if triple { 3 } else { 1 };
            continue;
        }

        if ch == b'#' {
            return None;
        }

        if ch == open {
            if open_idx.is_none() {
                open_idx = Some(idx);
            }
            depth += 1;
            idx += 1;
            continue;
        }

        if ch == close && depth > 0 {
            depth -= 1;
            if depth == 0 {
                return open_idx.map(|open| (open, idx));
            }
            idx += 1;
            continue;
        }

        idx += 1;
    }

    None
}

fn split_top_level_segments(inner: &str) -> Option<Vec<String>> {
    let bytes = inner.as_bytes();
    let mut idx = 0usize;
    let mut quote = None::<u8>;
    let mut escaped = false;
    let mut triple = false;
    let mut paren_depth = 0usize;
    let mut square_depth = 0usize;
    let mut curly_depth = 0usize;
    let mut start = 0usize;
    let mut segments = Vec::new();

    while idx < bytes.len() {
        let ch = bytes[idx];
        if let Some(q) = quote {
            if escaped {
                escaped = false;
                idx += 1;
                continue;
            }
            if triple
                && idx + 2 < bytes.len()
                && bytes[idx] == q
                && bytes[idx + 1] == q
                && bytes[idx + 2] == q
            {
                quote = None;
                triple = false;
                idx += 3;
                continue;
            }
            if ch == b'\\' && !triple {
                escaped = true;
            } else if ch == q && !triple {
                quote = None;
            }
            idx += 1;
            continue;
        }

        if ch == b'\'' || ch == b'"' {
            quote = Some(ch);
            triple = idx + 2 < bytes.len() && bytes[idx + 1] == ch && bytes[idx + 2] == ch;
            idx += if triple { 3 } else { 1 };
            continue;
        }

        match ch {
            b'(' => paren_depth += 1,
            b')' => paren_depth = paren_depth.saturating_sub(1),
            b'[' => square_depth += 1,
            b']' => square_depth = square_depth.saturating_sub(1),
            b'{' => curly_depth += 1,
            b'}' => curly_depth = curly_depth.saturating_sub(1),
            b',' if paren_depth == 0 && square_depth == 0 && curly_depth == 0 => {
                let segment = inner[start..idx].trim();
                if !segment.is_empty() {
                    segments.push(segment.to_string());
                }
                start = idx + 1;
            }
            _ => {}
        }

        idx += 1;
    }

    let tail = inner[start..].trim();
    if !tail.is_empty() {
        segments.push(tail.to_string());
    }
    if segments.is_empty() {
        None
    } else {
        Some(segments)
    }
}

fn needs_space_before_open_delimiter(prefix: &str, open: u8) -> bool {
    if open != b'{' {
        return false;
    }

    let Some(last) = prefix.as_bytes().last().copied() else {
        return false;
    };
    !matches!(last, b'(' | b'[' | b'{' | b' ')
}

fn needs_space_before_suffix(suffix: &str) -> bool {
    let Some(first) = suffix.as_bytes().first().copied() else {
        return false;
    };
    !matches!(first, b')' | b']' | b'}' | b',' | b'.' | b':' | b';')
}

fn normalize_comment(comment: &str) -> String {
    let Some(rest) = comment.strip_prefix('#') else {
        return comment.to_string();
    };
    let Some(first) = rest.chars().next() else {
        return "#".to_string();
    };
    if first.is_ascii_whitespace() || first == '#' {
        comment.to_string()
    } else {
        format!("# {}", rest)
    }
}

fn operator_at(bytes: &[u8], idx: usize) -> Option<&'static str> {
    const OPERATORS: [&str; 28] = [
        "<<=", ">>=", "->", ":=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "==", "!=", "<=",
        ">=", "<<", ">>", "&&", "||", "=", "+", "-", "*", "/", "%", "<", ">",
    ];

    for operator in OPERATORS {
        let op_bytes = operator.as_bytes();
        if idx + op_bytes.len() <= bytes.len() && &bytes[idx..idx + op_bytes.len()] == op_bytes {
            return Some(operator);
        }
    }
    None
}

fn skip_ascii_whitespace(bytes: &[u8], mut idx: usize) -> usize {
    while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
        idx += 1;
    }
    idx
}

fn should_emit_space_between(prev: u8, next: u8) -> bool {
    if next == b'#' {
        return false;
    }
    if prev == b'(' || prev == b'[' || prev == b'{' || prev == b'.' {
        return false;
    }
    if matches!(next, b')' | b']' | b'}' | b',' | b':' | b'.') {
        return false;
    }
    true
}

fn should_emit_space_before_operator(out: &str) -> bool {
    previous_significant_byte(out.as_bytes(), out.len())
        .is_some_and(|byte| !matches!(byte, b'(' | b'[' | b'{' | b'.'))
}

fn should_emit_space_after(bytes: &[u8], idx: usize) -> bool {
    let Some(next) = bytes.get(idx).copied() else {
        return false;
    };
    !matches!(next, b')' | b']' | b'}' | b',' | b';' | b':' | b'.' | b'#')
}

fn is_unary_sign(out: &str) -> bool {
    let Some(previous) = previous_significant_byte(out.as_bytes(), out.len()) else {
        return true;
    };
    if !is_expression_operand(previous) {
        return true;
    }

    previous_identifier(out).is_some_and(|ident| {
        matches!(
            ident,
            "return"
                | "await"
                | "assert"
                | "if"
                | "elif"
                | "while"
                | "for"
                | "in"
                | "and"
                | "or"
                | "not"
                | "var"
                | "const"
        )
    })
}

fn previous_identifier(out: &str) -> Option<&str> {
    let bytes = out.as_bytes();
    let end = previous_significant_index(bytes, bytes.len())?;
    if !is_identifier_byte(bytes[end]) {
        return None;
    }
    let mut start = end;
    while start > 0 && is_identifier_byte(bytes[start - 1]) {
        start -= 1;
    }
    out.get(start..=end)
}

fn previous_significant_byte(bytes: &[u8], from: usize) -> Option<u8> {
    let idx = previous_significant_index(bytes, from)?;
    Some(bytes[idx])
}

fn previous_significant_index(bytes: &[u8], from: usize) -> Option<usize> {
    if from == 0 {
        return None;
    }
    let mut idx = from;
    while idx > 0 {
        idx -= 1;
        if !bytes[idx].is_ascii_whitespace() {
            return Some(idx);
        }
    }
    None
}

fn is_expression_operand(byte: u8) -> bool {
    is_identifier_byte(byte) || matches!(byte, b')' | b']' | b'}' | b'"' | b'\'')
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn trim_trailing_spaces(out: &mut String) {
    while out.ends_with(' ') {
        out.pop();
    }
}

pub fn is_formatted(source: &str) -> bool {
    format_gdscript(source) == source
}

#[cfg(test)]
mod tests {
    use super::format_gdscript;

    #[test]
    fn normalizes_assignment_and_binary_operator_spacing() {
        let source = "func _physics_process(delta):\n    velocity.y -=         _gravity * delta\n    x:=1\n    speed<<=2\n";
        let expected = "func _physics_process(delta):\n    velocity.y -= _gravity * delta\n    x := 1\n    speed <<= 2\n";
        assert_eq!(format_gdscript(source), expected);
    }

    #[test]
    fn normalizes_commas_colons_and_return_arrow_spacing() {
        let source = "func move(v:Vector3)->void:\n    foo(a,b ,  c)\n    var d={\"hp\":100,\"name\" :\"Goblin\"}\n";
        let expected = "func move(v: Vector3) -> void:\n    foo(a, b, c)\n    var d = {\"hp\": 100, \"name\": \"Goblin\"}\n";
        assert_eq!(format_gdscript(source), expected);
    }

    #[test]
    fn preserves_unary_minus_without_extra_spacing() {
        let source = "func a(x):\n    return -   x\n";
        let expected = "func a(x):\n    return -x\n";
        assert_eq!(format_gdscript(source), expected);
    }

    #[test]
    fn collapses_multiple_blank_lines() {
        let source = "func a():\n    pass\n\n\n\nfunc b():\n    pass\n";
        let expected = "func a():\n    pass\n\nfunc b():\n    pass\n";
        assert_eq!(format_gdscript(source), expected);
    }

    #[test]
    fn does_not_rewrite_inside_strings_and_normalizes_inline_comment_spacing() {
        let source = "func a():\n    var s = \"x  +   y\"  #todo\n";
        let expected = "func a():\n    var s = \"x  +   y\" # todo\n";
        assert_eq!(format_gdscript(source), expected);
    }

    #[test]
    fn wraps_long_dictionary_record_lines() {
        let source = "func config() -> Dictionary:\n    var cfg = {\"gravity\": 9.8, \"jump_speed\": 14.0, \"air_control\": 0.35, \"camera_sensitivity\": 0.12, \"max_speed\": 35.0}\n";
        let expected = "func config() -> Dictionary:\n    var cfg = {\n        \"gravity\": 9.8,\n        \"jump_speed\": 14.0,\n        \"air_control\": 0.35,\n        \"camera_sensitivity\": 0.12,\n        \"max_speed\": 35.0\n    }\n";
        assert_eq!(format_gdscript(source), expected);
    }

    #[test]
    fn wraps_long_call_argument_lists() {
        let source = "func demo() -> void:\n    configure_player_state(\"run_fast\", 14.0, 0.35, 0.12, true, \"forward\", \"sprint\", 0.5, \"long_animation_name\", \"turn_accel_high\")\n";
        let expected = "func demo() -> void:\n    configure_player_state(\n        \"run_fast\",\n        14.0,\n        0.35,\n        0.12,\n        true,\n        \"forward\",\n        \"sprint\",\n        0.5,\n        \"long_animation_name\",\n        \"turn_accel_high\"\n    )\n";
        assert_eq!(format_gdscript(source), expected);
    }

    #[test]
    fn wraps_long_array_literals() {
        let source = "func points() -> Array:\n    var values = [Vector2(0.0, 0.0), Vector2(1.0, 1.0), Vector2(2.0, 2.0), Vector2(3.0, 3.0), Vector2(4.0, 4.0)]\n";
        let expected = "func points() -> Array:\n    var values = [\n        Vector2(0.0, 0.0),\n        Vector2(1.0, 1.0),\n        Vector2(2.0, 2.0),\n        Vector2(3.0, 3.0),\n        Vector2(4.0, 4.0)\n    ]\n";
        assert_eq!(format_gdscript(source), expected);
    }

    #[test]
    fn wraps_long_constructor_call_with_named_identifiers() {
        let source = "func _ready() -> void:\n    var random_pitch_offset := _rng.randf_range(-impact_pitch_randomness_super_long_name, impact_pitch_randomness_super_long_name)\n";
        let expected = "func _ready() -> void:\n    var random_pitch_offset := _rng.randf_range(\n        -impact_pitch_randomness_super_long_name,\n        impact_pitch_randomness_super_long_name\n    )\n";
        assert_eq!(format_gdscript(source), expected);
    }
}
