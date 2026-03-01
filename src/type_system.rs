use crate::docs_meta::{
    builtin_meta_header, class_meta_header, node_method_meta_header, validate_metadata_headers,
};
use crate::parser::{ParsedScript, ScriptDecl, ScriptDeclKind};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodSignature {
    pub name: String,
    pub class_name: String,
    pub signature: String,
    pub hover: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertySignature {
    pub name: String,
    pub class_name: String,
    pub property_type: String,
    pub documentation: String,
}

#[derive(Debug)]
struct ClassDoc {
    inherits: Vec<String>,
}

#[derive(Debug)]
struct TypeSystem {
    builtin_signatures: HashMap<String, (String, String)>,
    methods_by_name: HashMap<String, Vec<MethodSignature>>,
    class_metadata: HashMap<String, ClassDoc>,
}

impl TypeSystem {
    fn new() -> Self {
        Self {
            builtin_signatures: load_builtin_signatures(),
            methods_by_name: load_node_method_metadata(),
            class_metadata: load_class_metadata(),
        }
    }

    fn method_candidates_for_receiver(
        &self,
        receiver_type: Option<&str>,
        method_name: &str,
    ) -> Vec<MethodSignature> {
        let mut methods = self
            .methods_by_name
            .get(method_name)
            .cloned()
            .unwrap_or_default();

        if methods.is_empty() {
            return methods;
        }

        if let Some(receiver_type) = receiver_type {
            let ancestry = self.type_ancestry(receiver_type);
            let rank = ancestry
                .iter()
                .enumerate()
                .map(|(idx, ty)| (ty.clone(), idx))
                .collect::<HashMap<_, _>>();

            methods.retain(|method| rank.contains_key(&method.class_name));
            methods.sort_by(|a, b| {
                let rank_a = rank.get(&a.class_name).copied().unwrap_or(usize::MAX);
                let rank_b = rank.get(&b.class_name).copied().unwrap_or(usize::MAX);
                rank_a
                    .cmp(&rank_b)
                    .then(a.class_name.cmp(&b.class_name))
                    .then(a.signature.cmp(&b.signature))
            });
            methods.dedup_by(|a, b| {
                a.name == b.name && a.class_name == b.class_name && a.signature == b.signature
            });
            return methods;
        }

        methods.sort_by(|a, b| {
            a.class_name
                .cmp(&b.class_name)
                .then(a.signature.cmp(&b.signature))
        });
        methods.dedup_by(|a, b| {
            a.name == b.name && a.class_name == b.class_name && a.signature == b.signature
        });
        methods
    }

    fn type_ancestry(&self, ty: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut current = Some(ty.to_string());

        while let Some(name) = current {
            if !seen.insert(name.clone()) {
                break;
            }
            out.push(name.clone());
            current = self
                .class_metadata
                .get(&name)
                .and_then(|doc| doc.inherits.first())
                .cloned();
        }

        out
    }

    fn method_return_type(&self, receiver_type: &str, method_name: &str) -> Option<String> {
        let method = self
            .method_candidates_for_receiver(Some(receiver_type), method_name)
            .into_iter()
            .next()?;
        parse_signature_return_type(&method.signature)
    }

    fn property_signature_for_receiver(
        &self,
        receiver_type: &str,
        property_name: &str,
    ) -> Option<PropertySignature> {
        let getter_candidates = [
            format!("get_{property_name}"),
            format!("is_{property_name}"),
        ];

        let getter = getter_candidates.into_iter().find_map(|getter_name| {
            self.method_candidates_for_receiver(Some(receiver_type), &getter_name)
                .into_iter()
                .find(|method| parse_signature_return_type(&method.signature).is_some())
        });

        let setter = self
            .method_candidates_for_receiver(Some(receiver_type), &format!("set_{property_name}"))
            .into_iter()
            .find(|method| first_parameter_type(&method.signature).is_some());

        let property_type = getter
            .as_ref()
            .and_then(|method| parse_signature_return_type(&method.signature))
            .or_else(|| {
                setter
                    .as_ref()
                    .and_then(|method| first_parameter_type(&method.signature))
            })?;

        let class_name = getter
            .as_ref()
            .map(|method| method.class_name.clone())
            .or_else(|| setter.as_ref().map(|method| method.class_name.clone()))?;

        let documentation = getter
            .as_ref()
            .map(|method| method.hover.clone())
            .or_else(|| setter.as_ref().map(|method| method.hover.clone()))
            .unwrap_or_default();

        Some(PropertySignature {
            name: property_name.to_string(),
            class_name,
            property_type,
            documentation,
        })
    }
}

fn type_system() -> &'static TypeSystem {
    static TYPE_SYSTEM: OnceLock<TypeSystem> = OnceLock::new();
    TYPE_SYSTEM.get_or_init(TypeSystem::new)
}

pub fn method_return_type(receiver_type: &str, method_name: &str) -> Option<String> {
    type_system().method_return_type(receiver_type, method_name)
}

pub fn method_candidates_for_receiver(
    receiver_type: Option<&str>,
    method_name: &str,
) -> Vec<MethodSignature> {
    type_system().method_candidates_for_receiver(receiver_type, method_name)
}

pub fn builtin_signature(name: &str) -> Option<(String, String)> {
    type_system().builtin_signatures.get(name).cloned()
}

pub fn property_signature_for_receiver(
    receiver_type: &str,
    property_name: &str,
) -> Option<PropertySignature> {
    type_system().property_signature_for_receiver(receiver_type, property_name)
}

pub fn infer_expression_type(script: &ParsedScript, expr: &str, line: usize) -> Option<String> {
    let expr = expr.trim();
    if expr.is_empty() {
        return None;
    }

    if expr == "self" {
        return explicit_extends_in_file(script);
    }

    if is_type_name(expr) {
        return Some(expr.to_string());
    }

    if let Some(type_name) = constructor_type_from_expression(expr) {
        return Some(type_name);
    }

    let segments = split_member_chain(expr);
    let mut current_type = resolve_base_segment(script, segments.first()?.trim(), line)?;

    for segment in segments.iter().skip(1) {
        let segment = segment.trim();
        if segment.is_empty() {
            return None;
        }

        if segment.ends_with(']') {
            current_type = indexed_value_type(&current_type);
            continue;
        }

        if let Some(method_name) = method_name_from_segment(segment) {
            current_type = method_return_type(&current_type, &method_name)?;
            continue;
        }

        if let Some(property_name) = extract_identifier(segment).filter(|ident| ident == segment) {
            current_type = property_signature_for_receiver(&current_type, &property_name)
                .map(|property| property.property_type)?;
            continue;
        }

        return None;
    }

    Some(current_type)
}

pub fn infer_symbol_type(script: &ParsedScript, symbol: &str, line: usize) -> Option<String> {
    let symbol = symbol.trim();
    if symbol.is_empty() {
        return None;
    }

    if symbol == "self" {
        if let Some(extends) = explicit_extends_in_file(script) {
            return Some(extends);
        }
    }

    if let Some(assignment_type) = assignment_type_at(script, symbol, line) {
        return Some(assignment_type);
    }

    if let Some(param_type) = parameter_type_at(script, symbol, line) {
        return Some(param_type);
    }

    if let Some(decl) = best_matching_decl(script, symbol, line)
        && let Some(decl_type) = declaration_value_type(script, &decl)
    {
        return Some(decl_type);
    }

    if let Some(decl) = best_inline_binding_decl(script, symbol, line)
        && let Some(decl_type) = declaration_value_type(script, &decl)
    {
        return Some(decl_type);
    }

    if is_type_name(symbol) {
        return Some(symbol.to_string());
    }

    None
}

pub fn infer_literal_type(expr: &str) -> Option<String> {
    let expr = expr.trim();
    if expr.is_empty() {
        return None;
    }

    if expr == "true" || expr == "false" {
        return Some("bool".to_string());
    }
    if expr == "null" {
        return Some("Variant".to_string());
    }
    if expr.parse::<i64>().is_ok() {
        return Some("int".to_string());
    }
    if expr.parse::<f64>().is_ok() {
        return Some("float".to_string());
    }
    if (expr.starts_with('"') && expr.ends_with('"'))
        || (expr.starts_with('\'') && expr.ends_with('\''))
    {
        return Some("String".to_string());
    }
    if (expr.starts_with("&\"") && expr.ends_with('"'))
        || (expr.starts_with("&\'") && expr.ends_with('\''))
    {
        return Some("StringName".to_string());
    }
    if expr.starts_with('[') && expr.ends_with(']') {
        return Some("Array".to_string());
    }
    if expr.starts_with('{') && expr.ends_with('}') {
        return Some("Dictionary".to_string());
    }

    None
}

fn load_builtin_signatures() -> HashMap<String, (String, String)> {
    let source = include_str!("../data/godot_4_6_builtin_meta.tsv");
    validate_metadata_headers("godot_4_6_builtin_meta.tsv", source, builtin_meta_header())
        .unwrap_or_else(|error| panic!("{error}"));

    source
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut fields = line.splitn(3, '\t');
            let name = fields.next()?.trim();
            let signature = fields.next()?.trim();
            let hover = fields.next()?.trim();
            if name.is_empty() || signature.is_empty() || hover.is_empty() {
                None
            } else {
                Some((name.to_string(), (signature.to_string(), hover.to_string())))
            }
        })
        .collect()
}

fn load_node_method_metadata() -> HashMap<String, Vec<MethodSignature>> {
    let source = include_str!("../data/godot_4_6_node_method_meta.tsv");
    validate_metadata_headers(
        "godot_4_6_node_method_meta.tsv",
        source,
        node_method_meta_header(),
    )
    .unwrap_or_else(|error| panic!("{error}"));

    let mut methods = HashMap::new();
    for line in source.lines().skip(1) {
        let mut fields = line.splitn(4, '\t');
        let Some(name) = fields.next().map(str::trim) else {
            continue;
        };
        let Some(class_name) = fields.next().map(str::trim) else {
            continue;
        };
        let Some(signature) = fields.next().map(str::trim) else {
            continue;
        };
        let Some(hover) = fields.next().map(str::trim) else {
            continue;
        };

        if name.is_empty() || class_name.is_empty() || signature.is_empty() || hover.is_empty() {
            continue;
        }

        methods
            .entry(name.to_string())
            .or_insert_with(Vec::new)
            .push(MethodSignature {
                name: name.to_string(),
                class_name: class_name.to_string(),
                signature: signature.to_string(),
                hover: hover.to_string(),
            });
    }

    for methods in methods.values_mut() {
        methods.sort_by(|a, b| a.class_name.cmp(&b.class_name));
        methods.dedup_by(|a, b| {
            a.name == b.name && a.class_name == b.class_name && a.signature == b.signature
        });
    }

    methods
}

fn load_class_metadata() -> HashMap<String, ClassDoc> {
    let source = include_str!("../data/godot_4_6_class_meta.tsv");
    validate_metadata_headers("godot_4_6_class_meta.tsv", source, class_meta_header())
        .unwrap_or_else(|error| panic!("{error}"));

    let mut classes = HashMap::new();
    for line in source.lines().skip(1) {
        let mut fields = line.splitn(4, '\t');
        let Some(name) = fields.next().map(str::trim) else {
            continue;
        };
        let Some(inherits_raw) = fields.next().map(str::trim) else {
            continue;
        };

        if name.is_empty() {
            continue;
        }

        let inherits = inherits_raw
            .split('>')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();

        classes.insert(name.to_string(), ClassDoc { inherits });
    }

    classes
}

fn assignment_type_at(script: &ParsedScript, symbol: &str, line: usize) -> Option<String> {
    let target_scope = containing_function_line(script, line);
    let target_indent = script
        .lines
        .get(line.saturating_sub(1))
        .map(|line| line_indent(line))
        .unwrap_or(0);
    let mut best: Option<(usize, String)> = None;

    for (idx, raw_line) in script.lines.iter().enumerate().take(line) {
        let line_num = idx + 1;
        let assign_scope = containing_function_line(script, line_num);
        if assign_scope != target_scope {
            continue;
        }

        let Some((lhs, rhs)) = parse_simple_assignment(raw_line) else {
            continue;
        };
        if lhs != symbol {
            continue;
        }

        let assign_indent = line_indent(raw_line);
        if line_num < line && assign_indent > target_indent {
            continue;
        }

        let ty = infer_expression_type(script, rhs, line_num).or_else(|| infer_literal_type(rhs));
        if let Some(ty) = ty {
            if best
                .as_ref()
                .is_none_or(|(best_line, _)| line_num > *best_line)
            {
                best = Some((line_num, ty));
            }
        }
    }

    best.map(|(_, ty)| ty)
}

fn parse_simple_assignment(line_text: &str) -> Option<(String, &str)> {
    let code = parse_code_prefix(line_text).trim();
    if code.is_empty()
        || code.starts_with("var ")
        || code.starts_with("const ")
        || code.starts_with("func ")
    {
        return None;
    }

    if let Some((lhs, rhs)) = code.split_once(":=") {
        let lhs = lhs.trim();
        let symbol = extract_identifier(lhs)?;
        if symbol != lhs {
            return None;
        }
        return Some((symbol, rhs.trim()));
    }

    let (lhs, rhs) = code.split_once('=')?;
    let lhs = lhs.trim();
    let rhs = rhs.trim();
    if lhs.ends_with('!')
        || lhs.ends_with('<')
        || lhs.ends_with('>')
        || lhs.ends_with('=')
        || lhs.ends_with('+')
        || lhs.ends_with('-')
        || lhs.ends_with('*')
        || lhs.ends_with('/')
        || lhs.ends_with('%')
        || rhs.starts_with('=')
    {
        return None;
    }

    let symbol = extract_identifier(lhs)?;
    if symbol != lhs {
        return None;
    }
    Some((symbol, rhs))
}

fn best_matching_decl<'a>(
    script: &'a ParsedScript,
    symbol: &str,
    line: usize,
) -> Option<&'a ScriptDecl> {
    let target_scope = containing_function_line(script, line);
    let target_indent = script
        .lines
        .get(line.saturating_sub(1))
        .map(|line| line_indent(line))
        .unwrap_or(0);
    let target_scope_indent = target_scope.and_then(|scope_line| {
        script
            .lines
            .get(scope_line.saturating_sub(1))
            .map(|line| line_indent(line))
    });

    let mut matches = Vec::new();
    for decl in &script.declarations {
        if decl.name != symbol || decl.line > line {
            continue;
        }

        let Some(decl_indent) = script
            .lines
            .get(decl.line.saturating_sub(1))
            .map(|line| line_indent(line))
        else {
            continue;
        };
        if line > decl.line && decl_indent > target_indent {
            continue;
        }

        let decl_scope = containing_function_line(script, decl.line);
        if let (Some(target_scope_line), Some(decl_scope_line), Some(scope_indent)) =
            (target_scope, decl_scope, target_scope_indent)
        {
            if target_scope_line == decl_scope_line
                && !decl_visible_in_function_scope(script, decl.line, line, scope_indent)
            {
                continue;
            }
        }

        matches.push(decl);
    }

    if matches.is_empty() {
        return None;
    }

    matches.sort_by_key(|decl| {
        let decl_line = decl.line;
        let decl_scope = containing_function_line(script, decl_line);
        let scope_score = if decl_scope == target_scope { 0 } else { 1 };
        scope_score * 100_000 + decl_line
    });

    matches.into_iter().next()
}

fn declaration_value_type(script: &ParsedScript, decl: &ScriptDecl) -> Option<String> {
    match decl.kind {
        ScriptDeclKind::Variable | ScriptDeclKind::Constant => {
            let (_, code, _) = split_code_and_comment(&decl.text);
            parse_binding_type_and_value(script, &code, decl.line).0
        }
        ScriptDeclKind::Class => Some(decl.name.clone()),
        ScriptDeclKind::Function => None,
    }
}

fn parse_binding_type_and_value(
    script: &ParsedScript,
    code_line: &str,
    line: usize,
) -> (Option<String>, Option<String>) {
    let trimmed = strip_leading_annotations(code_line.trim_start());
    let rest = if let Some(rest) = trimmed.strip_prefix("var ") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("const ") {
        rest
    } else {
        return (None, None);
    };

    let (lhs, rhs) = if let Some((lhs, rhs)) = rest.split_once(":=") {
        (lhs.trim(), Some(rhs.trim()))
    } else if let Some((lhs, rhs)) = rest.split_once('=') {
        (lhs.trim(), Some(rhs.trim()))
    } else {
        (rest.trim(), None)
    };

    let declared_type = lhs
        .split_once(':')
        .map(|(_, ty)| ty.trim())
        .filter(|ty| !ty.is_empty())
        .map(str::to_string);

    let value = rhs
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let inferred_type = declared_type.or_else(|| {
        value
            .as_deref()
            .and_then(|expr| infer_expression_type(script, expr, line))
            .or_else(|| value.as_deref().and_then(infer_literal_type))
    });

    (inferred_type, value)
}

fn parameter_type_at(script: &ParsedScript, symbol: &str, line: usize) -> Option<String> {
    let function_line = containing_function_line(script, line)?;
    let signature = function_signature(script, function_line)?;
    let params = parse_function_parameters(&signature);
    let param = params.into_iter().find(|param| param.name == symbol)?;

    Some(
        param
            .param_type
            .or_else(|| param.default_value.as_deref().and_then(infer_literal_type))
            .unwrap_or_else(|| "Variant".to_string()),
    )
}

#[derive(Debug, Clone)]
struct FunctionParam {
    name: String,
    param_type: Option<String>,
    default_value: Option<String>,
}

fn function_signature(script: &ParsedScript, line: usize) -> Option<String> {
    let start = line.saturating_sub(1);
    if start >= script.lines.len() {
        return None;
    }

    let mut depth = 0isize;
    let mut saw_open = false;
    let mut chunks = Vec::new();

    for raw_line in script.lines.iter().skip(start) {
        let code = parse_code_prefix(raw_line).trim();
        if code.is_empty() {
            continue;
        }

        chunks.push(code.to_string());
        for ch in code.chars() {
            match ch {
                '(' => {
                    saw_open = true;
                    depth += 1;
                }
                ')' => depth -= 1,
                _ => {}
            }
        }

        if saw_open && depth <= 0 && code.contains(':') {
            break;
        }
    }

    if chunks.is_empty() {
        None
    } else {
        Some(chunks.join(" "))
    }
}

fn parse_function_parameters(signature: &str) -> Vec<FunctionParam> {
    let Some(open_idx) = signature.find('(') else {
        return Vec::new();
    };

    let mut depth = 0usize;
    let mut close_idx = None;
    for (idx, ch) in signature.char_indices().skip(open_idx) {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    close_idx = Some(idx);
                    break;
                }
            }
            _ => {}
        }
    }

    let Some(close_idx) = close_idx else {
        return Vec::new();
    };
    let params_src = signature[open_idx + 1..close_idx].trim();
    if params_src.is_empty() {
        return Vec::new();
    }

    split_top_level_commas(params_src)
        .into_iter()
        .filter_map(|segment| {
            let segment = segment.trim();
            if segment.is_empty() {
                return None;
            }

            let segment = segment.strip_prefix("...").unwrap_or(segment).trim();
            let (left, default_value) = if let Some((left, right)) = segment.split_once('=') {
                (left.trim(), Some(right.trim().to_string()))
            } else {
                (segment, None)
            };

            let (name_part, param_type) = if let Some((name, ty)) = left.split_once(':') {
                (
                    name.trim(),
                    Some(ty.trim().to_string()).filter(|value| !value.is_empty()),
                )
            } else {
                (left.trim(), None)
            };

            let name = extract_identifier(name_part)?;
            Some(FunctionParam {
                name,
                param_type,
                default_value,
            })
        })
        .collect()
}

fn function_return_type(signature: &str) -> Option<String> {
    parse_signature_return_type(signature)
}

fn parse_signature_return_type(signature: &str) -> Option<String> {
    let (_, tail) = signature.rsplit_once("->")?;
    let ty = tail.split(':').next().map(str::trim).unwrap_or_default();
    if ty.is_empty() {
        None
    } else if ty.eq_ignore_ascii_case("void") {
        None
    } else {
        Some(ty.to_string())
    }
}

fn first_parameter_type(signature: &str) -> Option<String> {
    let open_idx = signature.find('(')?;
    let mut depth = 0usize;
    let mut close_idx = None;
    for (idx, ch) in signature.char_indices().skip(open_idx) {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    close_idx = Some(idx);
                    break;
                }
            }
            _ => {}
        }
    }

    let close_idx = close_idx?;
    let params_src = signature[open_idx + 1..close_idx].trim();
    if params_src.is_empty() {
        return None;
    }

    let first = split_top_level_commas(params_src).into_iter().next()?;
    let first = first.trim();
    let first = first.strip_prefix("...").unwrap_or(first).trim();
    let first = first
        .split_once('=')
        .map(|(left, _)| left.trim())
        .unwrap_or(first);
    let (_, ty) = first.split_once(':')?;
    let ty = ty.trim();
    if ty.is_empty() {
        None
    } else {
        Some(ty.to_string())
    }
}

fn resolve_base_segment(script: &ParsedScript, segment: &str, line: usize) -> Option<String> {
    let segment = segment.trim();
    if segment.is_empty() {
        return None;
    }

    if let Some(type_name) = constructor_type_from_expression(segment) {
        return Some(type_name);
    }

    if let Some(index_base) = strip_index_suffix(segment) {
        return resolve_base_segment(script, index_base, line).map(|ty| indexed_value_type(&ty));
    }

    if let Some(call_name) = method_name_from_segment(segment) {
        if is_type_name(&call_name) {
            return Some(call_name);
        }

        if let Some(func_decl) = best_matching_decl(script, &call_name, line)
            && matches!(func_decl.kind, ScriptDeclKind::Function)
        {
            let signature = function_signature(script, func_decl.line).unwrap_or_default();
            if let Some(return_type) = function_return_type(&signature) {
                return Some(return_type);
            }
        }

        if let Some((signature, _)) = builtin_signature(&call_name) {
            return parse_signature_return_type(&signature);
        }

        return None;
    }

    infer_symbol_type(script, segment, line)
}

fn constructor_type_from_expression(expr: &str) -> Option<String> {
    let expr = expr.trim();
    let open = expr.find(".new(")?;
    let type_name = expr[..open].trim();
    if is_type_name(type_name) {
        Some(type_name.to_string())
    } else {
        None
    }
}

fn strip_index_suffix(segment: &str) -> Option<&str> {
    if !segment.ends_with(']') {
        return None;
    }

    let bytes = segment.as_bytes();
    let mut idx = bytes.len();
    let mut depth = 0isize;
    while idx > 0 {
        idx -= 1;
        match bytes[idx] {
            b']' => depth += 1,
            b'[' => {
                depth -= 1;
                if depth == 0 {
                    return Some(segment[..idx].trim());
                }
            }
            _ => {}
        }
    }
    None
}

fn indexed_value_type(container_type: &str) -> String {
    match container_type {
        "String" => "String".to_string(),
        "PackedByteArray" | "PackedInt32Array" | "PackedInt64Array" => "int".to_string(),
        "PackedStringArray" => "String".to_string(),
        "Array" | "Dictionary" => "Variant".to_string(),
        _ => "Variant".to_string(),
    }
}

fn method_name_from_segment(segment: &str) -> Option<String> {
    let segment = segment.trim();
    let open = segment.find('(')?;
    let name = segment[..open].trim();
    extract_identifier(name).filter(|ident| ident == name)
}

fn split_member_chain(expr: &str) -> Vec<String> {
    let bytes = expr.as_bytes();
    let mut idx = 0usize;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    let mut paren = 0usize;
    let mut bracket = 0usize;
    let mut brace = 0usize;
    let mut start = 0usize;
    let mut out = Vec::new();

    while idx < bytes.len() {
        let ch = bytes[idx];
        if let Some(q) = quote {
            if escaped {
                escaped = false;
                idx += 1;
                continue;
            }
            if ch == b'\\' {
                escaped = true;
                idx += 1;
                continue;
            }
            if ch == q {
                quote = None;
            }
            idx += 1;
            continue;
        }

        if ch == b'\'' || ch == b'"' {
            quote = Some(ch);
            idx += 1;
            continue;
        }

        match ch {
            b'(' => paren += 1,
            b')' => paren = paren.saturating_sub(1),
            b'[' => bracket += 1,
            b']' => bracket = bracket.saturating_sub(1),
            b'{' => brace += 1,
            b'}' => brace = brace.saturating_sub(1),
            b'.' if paren == 0 && bracket == 0 && brace == 0 => {
                let part = expr[start..idx].trim();
                if !part.is_empty() {
                    out.push(part.to_string());
                }
                start = idx + 1;
            }
            _ => {}
        }
        idx += 1;
    }

    let tail = expr[start..].trim();
    if !tail.is_empty() {
        out.push(tail.to_string());
    }
    out
}

fn split_top_level_commas(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for ch in input.chars() {
        if let Some(q) = quote {
            current.push(ch);
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == q {
                quote = None;
            }
            continue;
        }

        match ch {
            '\'' | '"' => {
                quote = Some(ch);
                current.push(ch);
            }
            '(' | '[' | '{' => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' | '}' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth == 0 => {
                out.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    if !current.trim().is_empty() {
        out.push(current.trim().to_string());
    }
    out
}

fn parse_code_prefix(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut idx = 0usize;
    let mut quote: Option<u8> = None;
    let mut escaped = false;

    while idx < bytes.len() {
        let ch = bytes[idx];
        match quote {
            Some(q) => {
                if escaped {
                    escaped = false;
                    idx += 1;
                    continue;
                }
                if ch == b'\\' {
                    escaped = true;
                    idx += 1;
                    continue;
                }
                if ch == q {
                    quote = None;
                }
                idx += 1;
            }
            None => {
                if ch == b'\'' || ch == b'"' {
                    quote = Some(ch);
                    idx += 1;
                    continue;
                }
                if ch == b'#' {
                    return line[..idx].trim_end();
                }
                idx += 1;
            }
        }
    }

    line.trim_end()
}

fn split_code_and_comment(line: &str) -> (String, String, Option<String>) {
    let bytes = line.as_bytes();
    let mut idx = 0usize;
    let mut quote: Option<u8> = None;
    let mut escaped = false;

    while idx < bytes.len() {
        let ch = bytes[idx];
        match quote {
            Some(q) => {
                if escaped {
                    escaped = false;
                    idx += 1;
                    continue;
                }
                if ch == b'\\' {
                    escaped = true;
                    idx += 1;
                    continue;
                }
                if ch == q {
                    quote = None;
                }
                idx += 1;
            }
            None => {
                if ch == b'\'' || ch == b'"' {
                    quote = Some(ch);
                    idx += 1;
                    continue;
                }
                if ch == b'#' {
                    let code = line[..idx].trim_end().to_string();
                    let comment = line[idx + 1..].trim().to_string();
                    return (line.to_string(), code, Some(comment));
                }
                idx += 1;
            }
        }
    }

    (line.to_string(), line.to_string(), None)
}

fn strip_leading_annotations(input: &str) -> &str {
    let mut rest = input.trim_start();
    loop {
        if !rest.starts_with('@') {
            return rest;
        }

        let bytes = rest.as_bytes();
        let mut idx = 1usize;
        while idx < bytes.len() && is_ident_char(bytes[idx]) {
            idx += 1;
        }
        if idx == 1 {
            return rest;
        }

        if idx < bytes.len() && bytes[idx] == b'(' {
            idx += 1;
            let mut depth = 1usize;
            let mut quote: Option<u8> = None;
            let mut escaped = false;

            while idx < bytes.len() {
                let ch = bytes[idx];
                match quote {
                    Some(q) => {
                        if escaped {
                            escaped = false;
                            idx += 1;
                            continue;
                        }
                        if ch == b'\\' {
                            escaped = true;
                            idx += 1;
                            continue;
                        }
                        if ch == q {
                            quote = None;
                        }
                        idx += 1;
                    }
                    None => {
                        if ch == b'\'' || ch == b'"' {
                            quote = Some(ch);
                            idx += 1;
                            continue;
                        }
                        if ch == b'(' {
                            depth += 1;
                        } else if ch == b')' {
                            depth = depth.saturating_sub(1);
                            idx += 1;
                            if depth == 0 {
                                break;
                            }
                            continue;
                        }
                        idx += 1;
                    }
                }
            }

            if depth != 0 {
                return rest;
            }
        }

        rest = rest[idx..].trim_start();
    }
}

fn parse_inline_binding_declaration(line_text: &str, line_num: usize) -> Option<ScriptDecl> {
    let code = parse_code_prefix(line_text).trim_start();
    let stripped = strip_leading_annotations(code);
    let (kind, rest) = if let Some(rest) = stripped.strip_prefix("var ") {
        (ScriptDeclKind::Variable, rest)
    } else if let Some(rest) = stripped.strip_prefix("const ") {
        (ScriptDeclKind::Constant, rest)
    } else {
        return None;
    };

    let name = extract_identifier(rest.trim_start())?;
    Some(ScriptDecl {
        kind,
        name,
        line: line_num,
        text: code.to_string(),
    })
}

fn best_inline_binding_decl(
    script: &ParsedScript,
    symbol: &str,
    line: usize,
) -> Option<ScriptDecl> {
    script
        .lines
        .iter()
        .enumerate()
        .take(line)
        .filter_map(|(idx, raw)| parse_inline_binding_declaration(raw, idx + 1))
        .filter(|decl| decl.name == symbol)
        .max_by_key(|decl| decl.line)
}

fn containing_function_line(script: &ParsedScript, target_line: usize) -> Option<usize> {
    let mut stack: Vec<(usize, usize)> = Vec::new();

    for (idx, raw_line) in script.lines.iter().enumerate() {
        let line_num = idx + 1;
        let trimmed = parse_code_prefix(raw_line).trim_start();
        let indent = line_indent(raw_line);

        while let Some((scope_line, scope_indent)) = stack.last().copied() {
            if line_num > scope_line && !trimmed.is_empty() && indent <= scope_indent {
                stack.pop();
                continue;
            }
            break;
        }

        if trimmed.starts_with("func ") {
            stack.push((line_num, indent));
            continue;
        }

        if line_num == target_line {
            return stack.last().map(|(scope_line, _)| *scope_line);
        }
    }

    None
}

fn decl_visible_in_function_scope(
    script: &ParsedScript,
    decl_line: usize,
    target_line: usize,
    function_indent: usize,
) -> bool {
    if decl_line > target_line {
        return false;
    }

    let decl_indent = script
        .lines
        .get(decl_line.saturating_sub(1))
        .map(|line| line_indent(line))
        .unwrap_or(usize::MAX);
    if decl_indent <= function_indent {
        return true;
    }

    for raw_line in script
        .lines
        .iter()
        .enumerate()
        .skip(decl_line)
        .take(target_line.saturating_sub(decl_line))
    {
        let line_text = parse_code_prefix(raw_line.1);
        let trimmed = line_text.trim_start();
        if trimmed.is_empty() {
            continue;
        }

        let indent = line_indent(raw_line.1);
        if indent < decl_indent {
            return true;
        }

        if indent == decl_indent && trimmed.starts_with("func ") {
            return false;
        }
    }

    true
}

fn line_indent(line: &str) -> usize {
    line.chars()
        .take_while(|ch| ch.is_ascii_whitespace())
        .count()
}

fn explicit_extends_in_file(script: &ParsedScript) -> Option<String> {
    script.lines.iter().find_map(|line| {
        let trimmed = parse_code_prefix(line).trim_start();
        let rest = trimmed.strip_prefix("extends ")?;
        let base = rest
            .split(|ch: char| ch.is_ascii_whitespace() || ch == '.')
            .next()
            .unwrap_or("")
            .trim();

        if base.is_empty() {
            None
        } else {
            Some(base.to_string())
        }
    })
}

fn is_type_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_uppercase() {
        return false;
    }
    let mut has_lower = false;
    for ch in chars.clone() {
        if ch.is_ascii_lowercase() {
            has_lower = true;
            break;
        }
    }
    if !has_lower {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn is_ident_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn extract_identifier(input: &str) -> Option<String> {
    let token = input
        .trim_start()
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect::<String>();
    if token.is_empty() {
        return None;
    }

    let mut chars = token.chars();
    let first = chars.next()?;
    if !first.is_ascii_alphabetic() && first != '_' {
        return None;
    }

    Some(token)
}

#[cfg(test)]
mod tests {
    use super::{
        builtin_signature, infer_expression_type, infer_symbol_type, method_return_type,
        property_signature_for_receiver,
    };
    use crate::parse_script;

    #[test]
    fn infer_symbols_from_declarations_assignments_and_parameters() {
        let source = "extends Node\nfunc _ready(level = 3):\n    var speed: float = 1.5\n    var count\n    count = 42\n    const Answer = 42\n";
        let script = parse_script(source, "inference_symbols.gd");

        assert_eq!(
            infer_symbol_type(&script, "speed", 4),
            Some("float".to_string())
        );
        assert_eq!(
            infer_symbol_type(&script, "self", 4),
            Some("Node".to_string())
        );
        assert_eq!(
            infer_symbol_type(&script, "level", 4),
            Some("int".to_string())
        );
        assert_eq!(
            infer_symbol_type(&script, "count", 5),
            Some("int".to_string())
        );
        assert_eq!(
            infer_symbol_type(&script, "Answer", 6),
            Some("int".to_string())
        );
    }

    #[test]
    fn infer_var_and_const_types_with_constructor_call() {
        let source = "func _ready():\n    var rng = RandomNumberGenerator.new()\n    const Answer = 42\n    return rng\n";
        let script = parse_script(source, "inference_constructor.gd");

        assert_eq!(
            infer_expression_type(&script, "RandomNumberGenerator.new()", 2),
            Some("RandomNumberGenerator".to_string())
        );
        assert_eq!(
            infer_symbol_type(&script, "rng", 2),
            Some("RandomNumberGenerator".to_string())
        );
        assert_eq!(
            infer_symbol_type(&script, "Answer", 3),
            Some("int".to_string())
        );
    }

    #[test]
    fn infer_chained_method_calls() {
        let source = "extends Node\nfunc _ready():\n    return self.get_tree().get_frame()\n";
        let script = parse_script(source, "inference_chain.gd");

        assert_eq!(
            infer_expression_type(&script, "self.get_tree().get_frame()", 3),
            Some("int".to_string())
        );
    }

    #[test]
    fn infer_chained_method_calls_after_local_function_return() {
        let source = "func _make_rng() -> RandomNumberGenerator:\n    return RandomNumberGenerator.new()\n\nfunc _ready() -> void:\n    _make_rng().randomize()\n";
        let script = parse_script(source, "inference_function_chain.gd");

        assert_eq!(
            infer_expression_type(&script, "_make_rng()", 5),
            Some("RandomNumberGenerator".to_string())
        );
        assert_eq!(
            infer_expression_type(&script, "_make_rng().randomize()", 5),
            None
        );
    }

    #[test]
    fn infer_chained_property_then_method_calls() {
        let source = "func _ready() -> void:\n    var _player: AudioStreamPlayer = AudioStreamPlayer.new()\n    _player.stream.get_length()\n";
        let script = parse_script(source, "inference_property_chain.gd");

        assert_eq!(
            infer_expression_type(&script, "_player.stream", 3),
            Some("AudioStream".to_string())
        );
        assert_eq!(
            infer_expression_type(&script, "_player.stream.get_length()", 3),
            Some("float".to_string())
        );

        let property = property_signature_for_receiver("AudioStreamPlayer", "stream")
            .expect("stream property");
        assert_eq!(property.class_name, "AudioStreamPlayer");
        assert_eq!(property.property_type, "AudioStream");
    }

    #[test]
    fn uses_metadata_for_method_return_lookup() {
        let method =
            method_return_type("Node", "get_tree").expect("get_tree should be in method metadata");
        assert_eq!(method, "SceneTree");

        let frame = method_return_type("SceneTree", "get_frame")
            .expect("SceneTree#get_frame should be in method metadata");
        assert_eq!(frame, "int");
    }

    #[test]
    fn loads_builtin_metadata() {
        let (signature, _) =
            builtin_signature("absf").expect("absf should exist in builtin metadata");
        assert!(signature.starts_with("absf("));
    }
}
