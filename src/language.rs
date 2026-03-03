#[derive(Debug, Clone, Copy)]
pub struct KeywordDoc {
    pub name: &'static str,
    pub snippet: &'static str,
    pub docs: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct AnnotationDoc {
    pub name: &'static str,
    pub snippet: &'static str,
    pub docs: &'static str,
}

pub const GDSCRIPT_KEYWORDS: &[&str] = &[
    "and",
    "as",
    "assert",
    "await",
    "break",
    "breakpoint",
    "class",
    "class_name",
    "const",
    "continue",
    "elif",
    "else",
    "enum",
    "extends",
    "for",
    "func",
    "if",
    "in",
    "is",
    "match",
    "not",
    "or",
    "pass",
    "preload",
    "return",
    "self",
    "signal",
    "static",
    "super",
    "var",
    "void",
    "when",
    "while",
];

pub const KNOWN_ANNOTATIONS: &[&str] = &[
    "deprecated",
    "experimental",
    "tutorial",
    "export",
    "export_category",
    "export_group",
    "export_subgroup",
    "export_storage",
    "export_custom",
    "export_color_no_alpha",
    "export_range",
    "export_enum",
    "export_exp_easing",
    "export_file",
    "export_file_path",
    "export_dir",
    "export_global_file",
    "export_global_dir",
    "export_multiline",
    "export_placeholder",
    "export_node_path",
    "export_flags",
    "export_flags_2d_render",
    "export_flags_2d_physics",
    "export_flags_2d_navigation",
    "export_flags_3d_render",
    "export_flags_3d_physics",
    "export_flags_3d_navigation",
    "export_flags_avoidance",
    "export_tool_button",
    "onready",
    "icon",
    "tool",
    "static_unload",
    "abstract",
    "rpc",
    "warning_ignore",
    "warning_ignore_start",
    "warning_ignore_restore",
];

const KEYWORD_DOCS: &[KeywordDoc] = &[
    KeywordDoc {
        name: "and",
        snippet: "and",
        docs: "Logical AND operator.",
    },
    KeywordDoc {
        name: "as",
        snippet: "value as Type",
        docs: "Casts a value to a target type.",
    },
    KeywordDoc {
        name: "assert",
        snippet: "assert(condition, message?)",
        docs: "Asserts that a condition is true.",
    },
    KeywordDoc {
        name: "await",
        snippet: "await expression",
        docs: "Waits for a coroutine or signal.",
    },
    KeywordDoc {
        name: "break",
        snippet: "break",
        docs: "Exits the nearest loop.",
    },
    KeywordDoc {
        name: "breakpoint",
        snippet: "breakpoint",
        docs: "Debugger breakpoint statement.",
    },
    KeywordDoc {
        name: "class",
        snippet: "class Name:",
        docs: "Declares an inner class.",
    },
    KeywordDoc {
        name: "class_name",
        snippet: "class_name Name",
        docs: "Registers the script as a global class.",
    },
    KeywordDoc {
        name: "const",
        snippet: "const NAME: Type = value",
        docs: "Declares a constant value.",
    },
    KeywordDoc {
        name: "continue",
        snippet: "continue",
        docs: "Skips to the next loop iteration.",
    },
    KeywordDoc {
        name: "elif",
        snippet: "elif condition:",
        docs: "Conditional branch after `if`.",
    },
    KeywordDoc {
        name: "else",
        snippet: "else:",
        docs: "Fallback branch for conditionals.",
    },
    KeywordDoc {
        name: "enum",
        snippet: "enum Name { A, B }",
        docs: "Declares an enum.",
    },
    KeywordDoc {
        name: "extends",
        snippet: "extends BaseType",
        docs: "Sets the base class for the script.",
    },
    KeywordDoc {
        name: "for",
        snippet: "for item in iterable:",
        docs: "Iterates over an iterable value.",
    },
    KeywordDoc {
        name: "func",
        snippet: "func name(args) -> Type:",
        docs: "Declares a function.",
    },
    KeywordDoc {
        name: "if",
        snippet: "if condition:",
        docs: "Starts a conditional branch.",
    },
    KeywordDoc {
        name: "in",
        snippet: "item in iterable",
        docs: "Membership and iteration operator.",
    },
    KeywordDoc {
        name: "is",
        snippet: "value is Type",
        docs: "Runtime type check.",
    },
    KeywordDoc {
        name: "match",
        snippet: "match value:",
        docs: "Pattern matching branch statement.",
    },
    KeywordDoc {
        name: "not",
        snippet: "not expression",
        docs: "Logical NOT operator.",
    },
    KeywordDoc {
        name: "or",
        snippet: "or",
        docs: "Logical OR operator.",
    },
    KeywordDoc {
        name: "pass",
        snippet: "pass",
        docs: "No-op placeholder statement.",
    },
    KeywordDoc {
        name: "preload",
        snippet: "preload(path)",
        docs: "Loads a resource at parse time and returns the resource.",
    },
    KeywordDoc {
        name: "return",
        snippet: "return value",
        docs: "Returns from a function.",
    },
    KeywordDoc {
        name: "self",
        snippet: "self",
        docs: "Reference to the current instance.",
    },
    KeywordDoc {
        name: "signal",
        snippet: "signal name(args...)",
        docs: "Declares a signal.",
    },
    KeywordDoc {
        name: "static",
        snippet: "static func name(...)",
        docs: "Marks members as class-level/static.",
    },
    KeywordDoc {
        name: "super",
        snippet: "super.method(...)",
        docs: "Calls a member on the parent implementation.",
    },
    KeywordDoc {
        name: "var",
        snippet: "var name: Type = value",
        docs: "Declares a mutable variable.",
    },
    KeywordDoc {
        name: "void",
        snippet: "-> void",
        docs: "Marks a function as returning no value.",
    },
    KeywordDoc {
        name: "when",
        snippet: "pattern when condition",
        docs: "Adds a guard condition in `match` patterns.",
    },
    KeywordDoc {
        name: "while",
        snippet: "while condition:",
        docs: "Loop while condition is true.",
    },
];

const ANNOTATION_DOCS: &[AnnotationDoc] = &[
    AnnotationDoc {
        name: "deprecated",
        snippet: "@deprecated",
        docs: "This annotation does not exist in Godot 4.x.",
    },
    AnnotationDoc {
        name: "experimental",
        snippet: "@experimental",
        docs: "This annotation does not exist in Godot 4.x.",
    },
    AnnotationDoc {
        name: "tutorial",
        snippet: "@tutorial",
        docs: "This annotation does not exist in Godot 4.x.",
    },
    AnnotationDoc {
        name: "export",
        snippet: "@export var value: Type",
        docs: "Exports a property to the Inspector.",
    },
    AnnotationDoc {
        name: "export_category",
        snippet: "@export_category(\"Category\")",
        docs: "Starts a top-level Inspector category for subsequent exported fields.",
    },
    AnnotationDoc {
        name: "export_group",
        snippet: "@export_group(\"Group\")",
        docs: "Starts an Inspector export group.",
    },
    AnnotationDoc {
        name: "export_subgroup",
        snippet: "@export_subgroup(\"Subgroup\")",
        docs: "Starts an Inspector export subgroup.",
    },
    AnnotationDoc {
        name: "export_storage",
        snippet: "@export_storage var value",
        docs: "Stores exported data without exposing it in the Inspector.",
    },
    AnnotationDoc {
        name: "export_custom",
        snippet: "@export_custom(...) var value",
        docs: "Defines custom export metadata for Inspector integration.",
    },
    AnnotationDoc {
        name: "export_color_no_alpha",
        snippet: "@export_color_no_alpha var color: Color",
        docs: "Exports a color picker without alpha.",
    },
    AnnotationDoc {
        name: "export_range",
        snippet: "@export_range(min, max, step?) var value: float",
        docs: "Exports a numeric property with slider/range metadata.",
    },
    AnnotationDoc {
        name: "export_enum",
        snippet: "@export_enum(\"A\", \"B\") var value: int",
        docs: "Exports enum options to the Inspector.",
    },
    AnnotationDoc {
        name: "export_exp_easing",
        snippet: "@export_exp_easing var value: float",
        docs: "Exports a float with exponential easing editor controls.",
    },
    AnnotationDoc {
        name: "export_file",
        snippet: "@export_file(\"*.ext\") var path: String",
        docs: "Exports a project file path selector.",
    },
    AnnotationDoc {
        name: "export_file_path",
        snippet: "@export_file_path(\"*.ext\") var path: String",
        docs: "Legacy alias for `@export_file`.",
    },
    AnnotationDoc {
        name: "export_dir",
        snippet: "@export_dir var path: String",
        docs: "Exports a project directory selector.",
    },
    AnnotationDoc {
        name: "export_global_file",
        snippet: "@export_global_file(\"*.ext\") var path: String",
        docs: "Exports a global file path selector.",
    },
    AnnotationDoc {
        name: "export_global_dir",
        snippet: "@export_global_dir var path: String",
        docs: "Exports a global directory selector.",
    },
    AnnotationDoc {
        name: "export_multiline",
        snippet: "@export_multiline var text: String",
        docs: "Exports a multiline text field.",
    },
    AnnotationDoc {
        name: "export_placeholder",
        snippet: "@export_placeholder(\"hint\") var text: String",
        docs: "Sets Inspector placeholder text for exported values.",
    },
    AnnotationDoc {
        name: "export_node_path",
        snippet: "@export_node_path(\"Type\") var target: NodePath",
        docs: "Exports a NodePath with optional type filtering.",
    },
    AnnotationDoc {
        name: "export_flags",
        snippet: "@export_flags(\"A\", \"B\") var mask: int",
        docs: "Exports an integer bitmask with named flag options.",
    },
    AnnotationDoc {
        name: "export_flags_2d_render",
        snippet: "@export_flags_2d_render var layers: int",
        docs: "Exports 2D render layer flags.",
    },
    AnnotationDoc {
        name: "export_flags_2d_physics",
        snippet: "@export_flags_2d_physics var layers: int",
        docs: "Exports 2D physics layer flags.",
    },
    AnnotationDoc {
        name: "export_flags_2d_navigation",
        snippet: "@export_flags_2d_navigation var layers: int",
        docs: "Exports 2D navigation layer flags.",
    },
    AnnotationDoc {
        name: "export_flags_3d_render",
        snippet: "@export_flags_3d_render var layers: int",
        docs: "Exports 3D render layer flags.",
    },
    AnnotationDoc {
        name: "export_flags_3d_physics",
        snippet: "@export_flags_3d_physics var layers: int",
        docs: "Exports 3D physics layer flags.",
    },
    AnnotationDoc {
        name: "export_flags_3d_navigation",
        snippet: "@export_flags_3d_navigation var layers: int",
        docs: "Exports 3D navigation layer flags.",
    },
    AnnotationDoc {
        name: "export_flags_avoidance",
        snippet: "@export_flags_avoidance var layers: int",
        docs: "Exports navigation avoidance layer flags.",
    },
    AnnotationDoc {
        name: "export_tool_button",
        snippet: "@export_tool_button(\"Label\") var action: Callable",
        docs: "Exports a callable shown as a button in the Inspector (tool scripts only).",
    },
    AnnotationDoc {
        name: "onready",
        snippet: "@onready var value = expr",
        docs: "Initializes the member after the node is ready in the scene tree.",
    },
    AnnotationDoc {
        name: "icon",
        snippet: "@icon(\"res://path.svg\")",
        docs: "Sets a custom icon for the script class.",
    },
    AnnotationDoc {
        name: "tool",
        snippet: "@tool",
        docs: "Runs the script in the editor.",
    },
    AnnotationDoc {
        name: "static_unload",
        snippet: "@static_unload",
        docs: "Requests static data unload when script is no longer used.",
    },
    AnnotationDoc {
        name: "abstract",
        snippet: "@abstract",
        docs: "Marks a class or method as abstract.",
    },
    AnnotationDoc {
        name: "rpc",
        snippet: "@rpc(...)",
        docs: "Configures RPC behavior for a method.",
    },
    AnnotationDoc {
        name: "warning_ignore",
        snippet: "@warning_ignore(\"code\")",
        docs: "Disables specific analyzer warnings for the next declaration or scope.",
    },
    AnnotationDoc {
        name: "warning_ignore_start",
        snippet: "@warning_ignore_start(\"code\")",
        docs: "Starts ignoring an analyzer warning until `@warning_ignore_restore`.",
    },
    AnnotationDoc {
        name: "warning_ignore_restore",
        snippet: "@warning_ignore_restore(\"code\")",
        docs: "Restores an analyzer warning ignored by `@warning_ignore_start`.",
    },
];

pub fn keyword_doc(name: &str) -> Option<&'static KeywordDoc> {
    KEYWORD_DOCS.iter().find(|entry| entry.name == name)
}

pub fn annotation_doc(name: &str) -> Option<&'static AnnotationDoc> {
    ANNOTATION_DOCS.iter().find(|entry| entry.name == name)
}

pub fn is_known_annotation(name: &str) -> bool {
    KNOWN_ANNOTATIONS.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::{
        GDSCRIPT_KEYWORDS, KNOWN_ANNOTATIONS, annotation_doc, is_known_annotation, keyword_doc,
    };

    #[test]
    fn keyword_docs_cover_all_gdscript_keywords() {
        for keyword in GDSCRIPT_KEYWORDS {
            assert!(
                keyword_doc(keyword).is_some(),
                "missing keyword hover docs for `{keyword}`"
            );
        }
    }

    #[test]
    fn annotation_docs_cover_known_annotations() {
        for annotation in KNOWN_ANNOTATIONS {
            assert!(
                annotation_doc(annotation).is_some(),
                "missing annotation hover docs for `@{annotation}`"
            );
            assert!(
                is_known_annotation(annotation),
                "known annotation helper rejected `@{annotation}`"
            );
        }
    }
}
