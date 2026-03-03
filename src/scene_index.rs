use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneScriptAttachment {
    pub scene_path: String,
    pub attached_node_path: String,
    pub attached_node_unique_name: Option<String>,
    pub child_node_paths: Vec<String>,
    pub child_unique_names: Vec<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SceneIndex {
    attachments_by_script: HashMap<String, Vec<SceneScriptAttachment>>,
}

impl SceneIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_scene(&mut self, scene_path: &str, scene_source: &str) {
        let scene_index = index_tscn_for_scene(scene_path, scene_source);
        for (script_path, attachments) in scene_index {
            self.attachments_by_script
                .entry(script_path)
                .or_default()
                .extend(attachments);
        }
    }

    pub fn attachments_for_script(&self, script_path: &str) -> &[SceneScriptAttachment] {
        self.attachments_by_script
            .get(script_path)
            .map_or(&[], Vec::as_slice)
    }

    pub fn scripts(&self) -> Vec<String> {
        let mut scripts = self.attachments_by_script.keys().cloned().collect::<Vec<_>>();
        scripts.sort_unstable();
        scripts
    }
}

#[derive(Debug, Clone, Default)]
struct RawNode {
    name: Option<String>,
    parent: Option<String>,
    unique_name_in_owner: bool,
    script_resource_id: Option<String>,
}

#[derive(Debug, Clone)]
struct ResolvedNode {
    name: String,
    path: String,
    unique_name_in_owner: bool,
    script_resource_id: Option<String>,
}

#[derive(Debug, Default, Clone)]
struct SceneParseState {
    ext_resources: HashMap<String, String>,
    nodes: Vec<RawNode>,
    current_node: Option<RawNode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SectionKind {
    ExtResource,
    Node,
    Other,
}

#[derive(Debug)]
struct ActiveSection {
    kind: SectionKind,
    ext_resource_id: Option<String>,
}

impl Default for ActiveSection {
    fn default() -> Self {
        Self {
            kind: SectionKind::Other,
            ext_resource_id: None,
        }
    }
}

pub fn index_tscn_for_scene(scene_path: &str, source: &str) -> HashMap<String, Vec<SceneScriptAttachment>> {
    let mut state = SceneParseState::default();
    let mut section = ActiveSection::default();

    for raw_line in source.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }

        if is_section_header(line) {
            finalize_current_node(&mut state, &mut section);
            process_section_header(line, &mut state, &mut section);
            continue;
        }

        match section.kind {
            SectionKind::Node => {
                if let Some((key, value)) = parse_assignment(line) {
                    apply_node_property(key, value, section.ext_resource_id.as_ref().map(|_| ()), &mut state.current_node);
                }
            }
            SectionKind::ExtResource => {}
            SectionKind::Other => {}
        }
    }

    finalize_current_node(&mut state, &mut section);
    build_attachments(scene_path, &state.ext_resources, &state.nodes)
}

fn process_section_header(line: &str, state: &mut SceneParseState, section: &mut ActiveSection) {
    let inner = &line[1..line.len() - 1];
    let mut parts = inner.splitn(2, char::is_whitespace);
    let section_type = parts.next().unwrap_or_default();

    match section_type {
        "ext_resource" => {
            section.kind = SectionKind::ExtResource;
            section.ext_resource_id = None;
            let attrs = parse_section_attrs(parts.next().unwrap_or_default());
            if let Some(id) = get_attr_string(&attrs, "id") {
                if let Some(path) = get_attr_string(&attrs, "path") {
                    state.ext_resources.insert(id, path);
                }
            }
        }
        "node" => {
            section.kind = SectionKind::Node;
            section.ext_resource_id = None;
            let attrs = parse_section_attrs(parts.next().unwrap_or_default());
            let mut node = RawNode::default();
            if let Some(name) = get_attr_string(&attrs, "name") {
                node.name = Some(name);
            }
            if let Some(parent) = get_attr_string(&attrs, "parent") {
                node.parent = Some(parent);
            }
            if let Some(unique) = get_attr_bool(&attrs, "unique_name_in_owner") {
                node.unique_name_in_owner = unique;
            }
            if let Some(script) = get_attr_string(&attrs, "script") {
                node.script_resource_id = parse_ext_resource_id(&script);
            }
            state.current_node = Some(node);
        }
        _ => {
            section.kind = SectionKind::Other;
            section.ext_resource_id = None;
        }
    }
}

fn finalize_current_node(state: &mut SceneParseState, section: &mut ActiveSection) {
    if section.kind == SectionKind::Node {
        if let Some(node) = state.current_node.take() {
            if node.name.is_some() {
                state.nodes.push(node);
            }
        }
    } else {
        state.current_node.take();
    }
    section.kind = SectionKind::Other;
    section.ext_resource_id = None;
}

fn apply_node_property(
    key: String,
    value: String,
    _ext_resource_guard: Option<()>,
    current_node: &mut Option<RawNode>,
) {
    let node = match current_node {
        Some(node) => node,
        None => return,
    };

    match key.as_str() {
        "name" => {
            node.name = Some(value);
        }
        "parent" => {
            node.parent = Some(value);
        }
        "unique_name_in_owner" => {
            if let Some(parsed) = parse_bool(value.as_str()) {
                node.unique_name_in_owner = parsed;
            }
        }
        "script" => {
            if let Some(resource_id) = parse_ext_resource_id(value.as_str()) {
                node.script_resource_id = Some(resource_id);
            }
        }
        _ => {}
    }
}

fn build_attachments(
    scene_path: &str,
    ext_resources: &HashMap<String, String>,
    nodes: &[RawNode],
) -> HashMap<String, Vec<SceneScriptAttachment>> {
    let mut resolved_nodes = Vec::<ResolvedNode>::with_capacity(nodes.len());
    let mut root_path: Option<String> = None;
    for node in nodes {
        let name = match &node.name {
            Some(name) => name,
            None => continue,
        };
        let parent = node.parent.clone().unwrap_or_else(|| ".".to_string());
        let path = build_node_path(parent.as_str(), name, root_path.as_deref());
        if root_path.is_none() {
            root_path = Some(path.clone());
        }
        resolved_nodes.push(ResolvedNode {
            name: name.clone(),
            path,
            unique_name_in_owner: node.unique_name_in_owner,
            script_resource_id: node.script_resource_id.clone(),
        });
    }

    let mut attachments = HashMap::<String, Vec<SceneScriptAttachment>>::new();
    for node in &resolved_nodes {
        let Some(resource_id) = node.script_resource_id.as_ref() else {
            continue;
        };
        let Some(script_path) = ext_resources.get(resource_id) else {
            continue;
        };

        let (child_paths, unique_child_names) = collect_children(&node.path, &resolved_nodes);
        attachments
            .entry(script_path.to_string())
            .or_default()
            .push(SceneScriptAttachment {
                scene_path: scene_path.to_string(),
                attached_node_path: node.path.clone(),
                attached_node_unique_name: if node.unique_name_in_owner {
                    Some(format!("%{}", node.name))
                } else {
                    None
                },
                child_node_paths: child_paths,
                child_unique_names: unique_child_names,
            });
    }

    attachments
}

fn collect_children(node_path: &str, nodes: &[ResolvedNode]) -> (Vec<String>, Vec<String>) {
    let prefix = format!("{node_path}/");
    let mut child_paths = Vec::new();
    let mut unique_names = Vec::new();

    for candidate in nodes {
        if candidate.path.as_str() == node_path {
            continue;
        }
        if !candidate.path.starts_with(&prefix) {
            continue;
        }
        child_paths.push(candidate.path.clone());
        if candidate.unique_name_in_owner {
            unique_names.push(format!("%{}", candidate.name));
        }
    }

    (child_paths, unique_names)
}

fn build_node_path(parent: &str, name: &str, root_path: Option<&str>) -> String {
    let normalized_parent = normalize_node_parent(parent);
    if normalized_parent.is_empty() {
        return match root_path {
            Some(root) if root != name => format!("{root}/{name}"),
            _ => name.to_string(),
        };
    }

    let full_parent = match root_path {
        Some(root)
            if normalized_parent != root && !normalized_parent.starts_with(&format!("{root}/")) =>
        {
            format!("{root}/{normalized_parent}")
        }
        _ => normalized_parent,
    };
    format!("{full_parent}/{name}")
}

fn normalize_node_parent(parent: &str) -> String {
    let mut value = parent.trim();
    if value.is_empty() || value == "." {
        return String::new();
    }

    while let Some(rest) = value.strip_prefix("./") {
        value = rest;
    }
    value.trim_matches('/').to_string()
}

fn is_section_header(line: &str) -> bool {
    line.starts_with('[') && line.ends_with(']')
}

fn parse_assignment(line: &str) -> Option<(String, String)> {
    let mut parts = split_once(line, '=');
    let key = parts.next()?.trim().to_string();
    let value = parse_value(parts.next()?.trim());
    Some((key, value))
}

fn parse_ext_resource_id(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if !trimmed.starts_with("ExtResource(") || !trimmed.ends_with(')') {
        return None;
    }
    let inner = &trimmed["ExtResource(".len()..trimmed.len() - 1];
    let inner = inner.trim();
    Some(parse_unquoted_or_quoted(inner))
}

fn parse_value(raw: &str) -> String {
    parse_unquoted_or_quoted(raw)
}

fn parse_unquoted_or_quoted(raw: &str) -> String {
    let value = raw.trim();
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        unescape(value[1..value.len() - 1].trim())
    } else {
        value.trim().to_string()
    }
}

fn parse_bool(raw: &str) -> Option<bool> {
    match raw.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut escaped = false;
    for ch in value.chars() {
        if escaped {
            out.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        out.push(ch);
    }
    out
}

fn parse_section_attrs(source: &str) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    let mut i = 0;
    let bytes = source.as_bytes();
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }

        let key_start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'=' {
            i += 1;
        }
        let key = source[key_start..i].trim().to_string();
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'=' {
            break;
        }
        i += 1; // '='

        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }

        let value = if bytes[i] == b'"' {
            i += 1;
            let start = i;
            let mut escaped = false;
            while i < bytes.len() {
                let ch = bytes[i];
                if escaped {
                    escaped = false;
                    i += 1;
                    continue;
                }
                if ch == b'\\' {
                    escaped = true;
                    i += 1;
                    continue;
                }
                if ch == b'"' {
                    break;
                }
                i += 1;
            }
            let raw = &source[start..i];
            let value = unescape(raw);
            if i < bytes.len() && bytes[i] == b'"' {
                i += 1;
            }
            value
        } else {
            let start = i;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            source[start..i].to_string()
        };

        attrs.push((key, value));
    }

    attrs
}

fn get_attr_string(attrs: &[(String, String)], key: &str) -> Option<String> {
    attrs
        .iter()
        .find_map(|(k, v)| if k == key { Some(v.clone()) } else { None })
}

fn get_attr_bool(attrs: &[(String, String)], key: &str) -> Option<bool> {
    attrs
        .iter()
        .find_map(|(k, v)| if k == key { parse_bool(v) } else { None })
}

fn split_once(s: &str, delim: char) -> SplitOnce {
    let mut parts = s.splitn(2, delim);
    SplitOnce {
        first: parts.next().map(|v| v.to_string()),
        second: parts.next().map(|v| v.to_string()),
    }
}

#[derive(Debug)]
struct SplitOnce {
    first: Option<String>,
    second: Option<String>,
}

impl Iterator for SplitOnce {
    type Item = String;
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(value) = self.first.take() {
            Some(value)
        } else {
            self.second.take()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_script_attachments_from_scene() {
        let source = r#"
[gd_scene load_steps=3 format=3]
[ext_resource type="Script" id="1" path="res://scripts/player.gd" format=3]
[node name="Player" type="Node3D" parent="."]
script = ExtResource("1")
[node name="Weapon" type="Node" parent="Player"]
script = ExtResource("1")
[node name="Barrel" type="Node3D" parent="Player/Weapon"]
[node name="NotAttached" type="Node3D" parent="."]

"#;

        let index = index_tscn_for_scene("res://scenes/player.tscn", source);
        let attachments = index.get("res://scripts/player.gd").unwrap();
        assert_eq!(attachments.len(), 2);

        let player = attachments
            .iter()
            .find(|a| a.attached_node_path == "Player")
            .expect("player node attachment");
        assert_eq!(player.scene_path, "res://scenes/player.tscn");
        assert_eq!(
            player.child_node_paths,
            vec![
                "Player/Weapon",
                "Player/Weapon/Barrel",
                "Player/NotAttached",
            ]
        );

        let weapon = attachments
            .iter()
            .find(|a| a.attached_node_path == "Player/Weapon")
            .expect("weapon node attachment");
        assert_eq!(weapon.child_node_paths, vec!["Player/Weapon/Barrel"]);
    }

    #[test]
    fn index_collects_unique_names() {
        let source = r#"
[gd_scene load_steps=2 format=3]
[ext_resource type="Script" id=2 path="res://scripts/scene.gd"]
[node name="Root" type="Node3D" parent="." unique_name_in_owner=true]
script = ExtResource(2)
[node name="Audio" type="AudioStreamPlayer3D" parent="Root" unique_name_in_owner = true]
[node name="Fx" type="Node3D" parent="Audio" unique_name_in_owner=true]
"#;

        let index = index_tscn_for_scene("res://scenes/unique.tscn", source);
        let attachments = index.get("res://scripts/scene.gd").unwrap();
        assert_eq!(attachments.len(), 1);

        let attachment = &attachments[0];
        assert_eq!(attachment.attached_node_path, "Root");
        assert_eq!(attachment.attached_node_unique_name, Some("%Root".to_string()));
        assert_eq!(
            attachment.child_node_paths,
            vec!["Root/Audio", "Root/Audio/Fx"]
        );
        assert_eq!(
            attachment.child_unique_names,
            vec!["%Audio", "%Fx"]
        );
    }

    #[test]
    fn accepts_script_ids_before_or_after_resource_definition() {
        let source = r#"
[gd_scene load_steps=2 format=3]
[node name="Root" type="Node3D" parent="."]
script = ExtResource("1")
[ext_resource type="Script" id="1" path="res://late/definition.gd"]
"#;

        let index = index_tscn_for_scene("res://scenes/later.tscn", source);
        assert!(index.contains_key("res://late/definition.gd"));
    }
}
