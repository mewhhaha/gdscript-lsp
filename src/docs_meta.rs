const CLASS_META_HEADER: &[&str] = &["name", "inherits", "summary", "note"];
const NODE_METHOD_META_HEADER: &[&str] = &["name", "class_name", "signature", "hover"];
const BUILTIN_META_HEADER: &[&str] = &["name", "signature", "hover"];

fn format_header_fields(fields: &[&str]) -> String {
    fields.join(", ")
}

pub fn validate_metadata_headers(
    source_path: &str,
    tsv: &str,
    expected: &[&str],
) -> Result<(), String> {
    let mut lines = tsv.lines();
    let Some(raw_header) = lines.next() else {
        return Err(format!(
            "{source_path}: missing header row; expected {}",
            format_header_fields(expected)
        ));
    };

    let header: Vec<&str> = raw_header
        .trim_end_matches('\r')
        .split('\t')
        .map(str::trim)
        .collect();

    if header.len() != expected.len() {
        return Err(format!(
            "{source_path}: invalid header width; expected {expected_count} columns, got {got_count}",
            expected_count = expected.len(),
            got_count = header.len()
        ));
    }

    for (idx, (expected_col, found_col)) in expected.iter().zip(header.iter()).enumerate() {
        if expected_col != found_col {
            return Err(format!(
                "{source_path}: invalid header at column {column}; expected `{expected}` but got `{found}`",
                column = idx + 1,
                expected = expected_col,
                found = found_col
            ));
        }
    }

    Ok(())
}

pub fn class_meta_header() -> &'static [&'static str] {
    CLASS_META_HEADER
}

pub fn node_method_meta_header() -> &'static [&'static str] {
    NODE_METHOD_META_HEADER
}

pub fn builtin_meta_header() -> &'static [&'static str] {
    BUILTIN_META_HEADER
}

#[cfg(test)]
mod tests {
    use super::{
        builtin_meta_header, class_meta_header, node_method_meta_header, validate_metadata_headers,
    };

    #[test]
    fn class_meta_header_accepts_expected_schema() {
        let source = "name\tinherits\tsummary\tnote";
        validate_metadata_headers("godot_4_6_class_meta.tsv", source, class_meta_header())
            .expect("class meta header should validate");
    }

    #[test]
    fn class_meta_header_rejects_missing_header() {
        let source = "";
        let err =
            validate_metadata_headers("godot_4_6_class_meta.tsv", source, class_meta_header())
                .expect_err("missing header should error");
        assert!(err.contains("missing header row"), "{err}");
    }

    #[test]
    fn node_method_header_accepts_expected_schema() {
        let source = "name\tclass_name\tsignature\thover";
        validate_metadata_headers(
            "godot_4_6_node_method_meta.tsv",
            source,
            node_method_meta_header(),
        )
        .expect("node method header should validate");
    }

    #[test]
    fn node_method_header_rejects_mismatched_schema() {
        let source = "name\tsignature\tclass_name\thover";
        let err = validate_metadata_headers(
            "godot_4_6_node_method_meta.tsv",
            source,
            node_method_meta_header(),
        )
        .expect_err("mismatched header should error");
        assert!(err.contains("invalid header at column 2"), "{err}");
    }

    #[test]
    fn builtin_meta_header_accepts_expected_schema() {
        let source = "name\tsignature\thover";
        validate_metadata_headers("godot_4_6_builtin_meta.tsv", source, builtin_meta_header())
            .expect("builtin header should validate");
    }
}
