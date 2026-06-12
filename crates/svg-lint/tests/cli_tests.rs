//! CLI smoke / golden tests for the `svg-lint` binary.
//!
//! These exercise the binary end-to-end (argument parsing, stdin/file
//! input, output formats, exit codes) without re-testing the library's
//! lint logic, which has its own unit tests.

use std::{
    error::Error,
    io::Write,
    process::{Command, Stdio},
};

/// Path to the freshly-built `svg-lint` binary, provided by Cargo.
const BIN: &str = env!("CARGO_BIN_EXE_svg-lint");

/// Convenient alias for fallible tests.
type TestResult = Result<(), Box<dyn Error>>;

/// Outcome of invoking the binary: captured streams plus exit code.
struct Output {
    stdout: String,
    stderr: String,
    code: i32,
}

/// Run the binary with `args` and optional `stdin`, capturing output.
fn run(args: &[&str], stdin: Option<&str>) -> Result<Output, Box<dyn Error>> {
    let mut child = Command::new(BIN)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(input) = stdin {
        let mut handle = child.stdin.take().ok_or("child stdin unavailable")?;
        handle.write_all(input.as_bytes())?;
        // Drop the handle so the child sees EOF before we wait.
        drop(handle);
    }
    let output = child.wait_with_output()?;
    Ok(Output {
        stdout: String::from_utf8(output.stdout)?,
        stderr: String::from_utf8(output.stderr)?,
        code: output.status.code().ok_or("process terminated by signal")?,
    })
}

#[test]
fn valid_svg_from_stdin_is_clean() -> TestResult {
    let out = run(
        &["--stdin"],
        Some(r#"<svg><rect width="1" height="1"/></svg>"#),
    )?;
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(
        out.stdout.contains("No diagnostics."),
        "stdout: {}",
        out.stdout
    );
    Ok(())
}

#[test]
fn dash_reads_stdin() -> TestResult {
    let out = run(&["-"], Some("<svg><banana/></svg>"))?;
    assert_eq!(out.code, 1, "stderr: {}", out.stderr);
    assert!(out.stdout.contains("<stdin>"), "stdout: {}", out.stdout);
    assert!(
        out.stdout.contains("UnknownElement"),
        "stdout: {}",
        out.stdout
    );
    Ok(())
}

#[test]
fn unknown_element_text_output_has_line_col_and_code() -> TestResult {
    let out = run(&["--stdin"], Some("<svg><banana/></svg>"))?;
    assert_eq!(out.code, 1, "stderr: {}", out.stderr);
    // 1-based line:col; <banana> opens at byte 6 => column 7.
    assert!(
        out.stdout.contains("1:7 error [UnknownElement]"),
        "stdout: {}",
        out.stdout
    );
    Ok(())
}

#[test]
fn json_output_is_machine_readable() -> TestResult {
    let out = run(
        &["--stdin", "--format", "json"],
        Some("<svg><banana/></svg>"),
    )?;
    assert_eq!(out.code, 1, "stderr: {}", out.stderr);
    let value: serde_json::Value = serde_json::from_str(&out.stdout)?;
    let files = value.as_array().ok_or("top-level value is not an array")?;
    assert_eq!(files.len(), 1);
    let diag = &files[0]["diagnostics"][0];
    assert_eq!(diag["code"], "UnknownElement");
    assert_eq!(diag["severity"], "error");
    assert_eq!(diag["line"], 1);
    assert_eq!(diag["range"]["startRow"], 0);
    Ok(())
}

#[test]
fn unknown_profile_is_usage_error() -> TestResult {
    let out = run(&["--profile", "bogus", "--stdin"], Some("<svg/>"))?;
    assert_eq!(out.code, 2, "stdout: {}", out.stdout);
    assert!(
        out.stderr.contains("unknown profile 'bogus'"),
        "stderr: {}",
        out.stderr
    );
    Ok(())
}

#[test]
fn profile_alias_resolves_and_affects_linting() -> TestResult {
    // xlink:href is clean under SVG 1.1 but unsupported under SVG 2.
    let src = r##"<svg xmlns:xlink="http://www.w3.org/1999/xlink"><defs><g id="i"/></defs><use xlink:href="#i"/></svg>"##;

    let svg11 = run(
        &["--stdin", "--profile", "svg1.1", "--force-profile"],
        Some(src),
    )?;
    assert_eq!(svg11.code, 0, "svg1.1 stderr: {}", svg11.stderr);

    let svg2 = run(
        &["--stdin", "--profile", "svg2", "--force-profile"],
        Some(src),
    )?;
    assert_eq!(svg2.code, 1, "svg2 stdout: {}", svg2.stdout);
    assert!(
        svg2.stdout.contains("UnsupportedInProfile"),
        "svg2 stdout: {}",
        svg2.stdout
    );
    Ok(())
}

#[test]
fn max_severity_error_passes_on_warning_only_document() -> TestResult {
    // A missing url(#...) reference is a Warning, below the `error`
    // threshold, so the gate passes (exit 0) while the diagnostic is
    // still printed.
    let out = run(
        &["--stdin", "--max-severity", "error"],
        Some(r#"<svg><rect clip-path="url(#x)"/></svg>"#),
    )?;
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(
        out.stdout.contains("MissingReferenceDefinition"),
        "stdout: {}",
        out.stdout
    );
    Ok(())
}

#[test]
fn quiet_hides_diagnostics_below_threshold() -> TestResult {
    let out = run(
        &["--stdin", "--max-severity", "error", "--quiet"],
        Some(r#"<svg><rect clip-path="url(#x)"/></svg>"#),
    )?;
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(
        out.stdout.contains("No diagnostics."),
        "stdout: {}",
        out.stdout
    );
    assert!(
        !out.stdout.contains("MissingReferenceDefinition"),
        "stdout: {}",
        out.stdout
    );
    Ok(())
}

#[test]
fn mixing_stdin_token_with_files_is_rejected() -> TestResult {
    let out = run(&["-", "some-file.svg"], Some("<svg/>"))?;
    assert_eq!(out.code, 2, "stdout: {}", out.stdout);
    assert!(out.stderr.contains("cannot mix"), "stderr: {}", out.stderr);
    Ok(())
}

#[test]
fn missing_file_is_io_error() -> TestResult {
    let out = run(&["definitely-does-not-exist.svg"], None)?;
    assert_eq!(out.code, 2, "stdout: {}", out.stdout);
    assert!(
        out.stderr.contains("failed reading"),
        "stderr: {}",
        out.stderr
    );
    Ok(())
}
