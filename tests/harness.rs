use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

const INPUT_PLACEHOLDER: &str = "${fixture}";
const FIXTURE_DIR_PLACEHOLDER: &str = "${fixture_dir}";
const FIXTURE_OUTPUT_PLACEHOLDER: &str = "__FIXTURE__";

#[derive(Debug)]
pub struct Fixture {
    root: PathBuf,
    suite: &'static str,
    name: &'static str,
    input_path: PathBuf,
}

impl Fixture {
    pub fn new(suite: &'static str, name: &'static str) -> Self {
        let root = fixture_dir(suite, name);
        Self {
            root,
            suite,
            name,
            input_path: fixture_path(suite, name, "input.gd"),
        }
    }

    pub fn args(&self) -> Vec<String> {
        let mut args = load_lines(&self.root, "args.txt").collect::<Vec<_>>();
        if !args.iter().any(|arg| arg.contains(INPUT_PLACEHOLDER)) && self.input_path.exists() {
            args.push(self.input_path.to_string_lossy().into_owned());
        }

        args.into_iter()
            .map(|arg| {
                arg.replace(INPUT_PLACEHOLDER, &self.input_path.to_string_lossy())
                    .replace(FIXTURE_DIR_PLACEHOLDER, &self.root.to_string_lossy())
            })
            .collect()
    }

    pub fn expected_exit_code(&self) -> i32 {
        let text = read_text(self.root.join("expect").join("exit.txt"));
        text.trim().parse::<i32>().unwrap_or(0)
    }

    pub fn expected_stdout(&self) -> String {
        normalize_trim_newline(read_text_if_exists(
            self.root.join("expect").join("stdout.golden"),
        ))
    }

    pub fn expected_stderr(&self) -> String {
        normalize_trim_newline(read_text_if_exists(
            self.root.join("expect").join("stderr.golden"),
        ))
    }
}

pub fn run_fixture_case(suite: &'static str, name: &'static str) {
    let fixture = Fixture::new(suite, name);
    let binary = resolve_binary_path();
    let args = fixture.args();

    let output = Command::new(binary)
        .args(&args)
        .current_dir(env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .output()
        .expect("failed to run cli");

    let stdout = normalize_trim_newline(normalize_output(
        &String::from_utf8_lossy(&output.stdout),
        &fixture,
    ));
    let stderr = normalize_trim_newline(normalize_output(
        &String::from_utf8_lossy(&output.stderr),
        &fixture,
    ));

    assert_eq!(
        output.status.code().unwrap_or(-1),
        fixture.expected_exit_code(),
        "CLI exit mismatch for fixture `{}/{}`\nstdout:\n{}\nstderr:\n{}",
        fixture.suite,
        fixture.name,
        stdout,
        stderr
    );

    assert_eq!(
        stdout,
        fixture.expected_stdout(),
        "stdout mismatch for fixture `{}/{}`",
        fixture.suite,
        fixture.name
    );

    assert_eq!(
        stderr,
        fixture.expected_stderr(),
        "stderr mismatch for fixture `{}/{}`",
        fixture.suite,
        fixture.name
    );
}

pub fn run_cli_with_args(args: &[&str], working_dir: Option<&Path>) -> (i32, String, String) {
    let binary = resolve_binary_path();
    let cwd = working_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let output = Command::new(binary)
        .args(args)
        .current_dir(&cwd)
        .output()
        .expect("failed to run cli");

    (
        output.status.code().unwrap_or(-1),
        normalize_trim_newline(String::from_utf8_lossy(&output.stdout).to_string()),
        normalize_trim_newline(String::from_utf8_lossy(&output.stderr).to_string()),
    )
}

fn resolve_binary_path() -> PathBuf {
    if let Ok(path) = env::var("CARGO_BIN_EXE_gdscript-lsp") {
        return PathBuf::from(path);
    }
    if let Ok(path) = env::var("CARGO_BIN_EXE_gdscript_lsp") {
        return PathBuf::from(path);
    }

    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target");
    path.push("debug");
    path.push(if cfg!(windows) {
        "gdscript-lsp.exe"
    } else {
        "gdscript-lsp"
    });
    path
}

fn fixture_root_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

fn fixture_dir(suite: &str, name: &str) -> PathBuf {
    fixture_root_dir().join(suite).join(name)
}

fn fixture_path(suite: &str, name: &str, file: &str) -> PathBuf {
    fixture_dir(suite, name).join(file)
}

fn read_text(path: PathBuf) -> String {
    fs::read_to_string(path)
        .unwrap_or_default()
        .replace('\r', "\n")
}

fn read_text_if_exists(path: PathBuf) -> String {
    if path.exists() {
        read_text(path)
    } else {
        String::new()
    }
}

fn load_lines(dir: &Path, file: &str) -> std::vec::IntoIter<String> {
    let contents = read_text_if_exists(dir.join(file));
    let lines = contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
    lines.into_iter()
}

fn normalize_output(output: &str, fixture: &Fixture) -> String {
    let input = fixture.input_path.to_string_lossy().replace('\\', "/");
    let fixture_root = fixture.root.to_string_lossy().replace('\\', "/");
    output
        .replace('\r', "\n")
        .replace(&input, FIXTURE_OUTPUT_PLACEHOLDER)
        .replace(&fixture_root, "__FIXTURE_DIR__")
}

fn normalize_trim_newline(text: String) -> String {
    text.replace('\r', "\n").trim_end_matches('\n').to_string()
}
