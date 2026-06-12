//! Command-line entrypoint for the `svg-lint` linter.
//!
//! Lints one or more SVG files (or stdin) against the generated SVG
//! catalog and prints transport-agnostic diagnostics in either a
//! human-readable or JSON form. The binary is a thin shell over the
//! `svg_lint` library: it parses each input once, resolves the spec
//! profile, and renders whatever diagnostics the library returns. No
//! lint logic lives here.

use std::{
    fmt::Write as _,
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::{Parser, ValueEnum};
use svg_lint::{LintOptions, Severity, SvgDiagnostic};
use tree_sitter::Parser as TsParser;

/// Exit code for usage and I/O errors (distinct from lint failures).
const EXIT_USAGE: u8 = 2;
/// Exit code when diagnostics at or above the failure threshold are found.
const EXIT_LINT_FAILURE: u8 = 1;

#[derive(Debug, Parser)]
#[command(
    name = "svg-lint",
    version,
    about = "Structural linter for SVG documents",
    long_about = "Validates SVG documents against the generated SVG catalog and prints\n\
diagnostics grouped by file. Reads one or more FILES, or stdin when `-`\n\
or --stdin is given. The spec profile is taken from each document's own\n\
`version`/`baseProfile` declaration when present, falling back to\n\
--profile; pass --force-profile to ignore the document and always use\n\
--profile."
)]
struct Cli {
    /// SVG files to lint. Use `-` to read from stdin.
    #[arg(value_name = "FILE")]
    paths: Vec<PathBuf>,

    /// Read a single SVG document from stdin (equivalent to passing `-`).
    #[arg(long, conflicts_with = "paths")]
    stdin: bool,

    /// Output format for diagnostics.
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,

    /// Spec profile/snapshot to lint against when a document declares none.
    ///
    /// Accepts canonical ids and friendly aliases (e.g. `svg2`, `svg1.1`).
    #[arg(long, value_name = "PROFILE", default_value = "Svg2EditorsDraft")]
    profile: String,

    /// Ignore each document's declared profile and always use --profile.
    #[arg(long)]
    force_profile: bool,

    /// Lowest severity that causes a non-zero exit (default: warning).
    #[arg(long, value_enum, default_value_t = SeverityArg::Warning)]
    max_severity: SeverityArg,

    /// Suppress diagnostics below the --max-severity threshold from output.
    #[arg(short, long)]
    quiet: bool,
}

/// Diagnostic rendering format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    /// Human-readable, grouped by file.
    Text,
    /// Machine-readable JSON (an array of file results).
    Json,
}

/// Severity threshold selector for `--max-severity`.
///
/// Ordered weakest → strongest so a threshold admits everything at or
/// above it. Mirrors LSP severities; `none` disables the failure gate so
/// the linter only ever exits non-zero on usage/I/O errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum SeverityArg {
    /// Never fail on diagnostics; only usage/I/O errors are fatal.
    None,
    /// Fail on hints and stronger.
    Hint,
    /// Fail on informational diagnostics and stronger.
    Info,
    /// Fail on warnings and stronger.
    Warning,
    /// Fail only on errors.
    Error,
}

/// Numeric rank for a library [`Severity`], strongest = highest.
///
/// The library `Severity` deliberately carries no `Ord`; the CLI owns the
/// threshold ordering so the library stays presentation-agnostic.
const fn severity_rank(severity: Severity) -> u8 {
    match severity {
        Severity::Hint => 1,
        Severity::Information => 2,
        Severity::Warning => 3,
        Severity::Error => 4,
    }
}

impl SeverityArg {
    /// Minimum [`severity_rank`] that trips the failure gate, or `None`
    /// when the gate is disabled.
    const fn threshold_rank(self) -> Option<u8> {
        match self {
            Self::None => None,
            Self::Hint => Some(1),
            Self::Info => Some(2),
            Self::Warning => Some(3),
            Self::Error => Some(4),
        }
    }

    /// Whether `severity` meets or exceeds this threshold.
    const fn admits(self, severity: Severity) -> bool {
        match self.threshold_rank() {
            None => false,
            Some(rank) => severity_rank(severity) >= rank,
        }
    }
}

/// Stable lowercase label for a severity, shared by text and JSON output.
const fn severity_label(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Information => "info",
        Severity::Hint => "hint",
    }
}

/// One linted input: where it came from plus its diagnostics.
struct FileReport {
    /// Display label (`<stdin>` or the file path).
    label: String,
    /// Diagnostics in the order the library produced them.
    diagnostics: Vec<SvgDiagnostic>,
}

/// A resolved input source paired with its bytes.
enum Source {
    /// Standard input.
    Stdin,
    /// A file at the given path.
    File(PathBuf),
}

impl Source {
    /// Human-facing label for grouping output.
    fn label(&self) -> String {
        match self {
            Self::Stdin => "<stdin>".to_string(),
            Self::File(path) => path.display().to_string(),
        }
    }
}

fn run() -> Result<ExitCode, String> {
    let cli = Cli::parse();

    let configured_profile = svg_data::resolve_profile_id(&cli.profile).ok_or_else(|| {
        format!(
            "unknown profile '{}'. Known profiles: {}",
            cli.profile,
            known_profiles().join(", ")
        )
    })?;

    let sources = resolve_sources(&cli)?;

    let mut reports = Vec::with_capacity(sources.len());
    for source in &sources {
        let bytes = read_source(source)?;
        let diagnostics = lint_bytes(&bytes, configured_profile, cli.force_profile);
        reports.push(FileReport {
            label: source.label(),
            diagnostics,
        });
    }

    let mut stdout = io::stdout().lock();
    match cli.format {
        OutputFormat::Text => write_text(&mut stdout, &reports, cli.max_severity, cli.quiet),
        OutputFormat::Json => write_json(&mut stdout, &reports, cli.max_severity, cli.quiet),
    }
    .map_err(|err| format!("failed writing output: {err}"))?;

    let failed = reports.iter().any(|report| {
        report
            .diagnostics
            .iter()
            .any(|diag| cli.max_severity.admits(diag.severity))
    });

    Ok(if failed {
        ExitCode::from(EXIT_LINT_FAILURE)
    } else {
        ExitCode::SUCCESS
    })
}

/// Turn CLI inputs into an ordered list of sources, rejecting ambiguous
/// combinations (multiple stdin reads, mixing `-` with files).
fn resolve_sources(cli: &Cli) -> Result<Vec<Source>, String> {
    if cli.stdin {
        // --stdin already conflicts with positional paths via clap.
        return Ok(vec![Source::Stdin]);
    }

    if cli.paths.is_empty() {
        return Ok(vec![Source::Stdin]);
    }

    let stdin_requested = cli.paths.iter().any(|path| is_stdin_token(path));
    if stdin_requested {
        if cli.paths.len() > 1 {
            return Err("cannot mix '-' (stdin) with file arguments".to_string());
        }
        return Ok(vec![Source::Stdin]);
    }

    Ok(cli
        .paths
        .iter()
        .map(|path| Source::File(path.clone()))
        .collect())
}

/// Whether a positional argument names stdin (`-`).
fn is_stdin_token(path: &Path) -> bool {
    path.as_os_str() == "-"
}

/// Read a source's bytes, surfacing I/O failures with the input named.
fn read_source(source: &Source) -> Result<Vec<u8>, String> {
    match source {
        Source::Stdin => {
            let mut buf = Vec::new();
            io::stdin()
                .read_to_end(&mut buf)
                .map_err(|err| format!("failed reading stdin: {err}"))?;
            Ok(buf)
        }
        Source::File(path) => {
            fs::read(path).map_err(|err| format!("failed reading '{}': {err}", path.display()))
        }
    }
}

/// Parse `bytes` once and lint against the effective profile.
///
/// Resolves the document's declared `version`/`baseProfile` profile (the
/// same rule the LSP applies) unless `force_profile` pins `configured`.
/// On a parse failure the library yields no tree; we mirror its empty
/// result rather than inventing diagnostics.
fn lint_bytes(
    bytes: &[u8],
    configured: svg_data::SpecSnapshotId,
    force_profile: bool,
) -> Vec<SvgDiagnostic> {
    let mut parser = TsParser::new();
    if parser
        .set_language(&tree_sitter_svg::LANGUAGE.into())
        .is_err()
    {
        // The grammar is compiled into the binary; an ABI mismatch here is
        // an internal build error, but surface it as no diagnostics rather
        // than panicking on user input.
        return Vec::new();
    }
    let Some(tree) = parser.parse(bytes, None) else {
        return Vec::new();
    };
    let profile = svg_lint::effective_profile(&tree, bytes, configured, force_profile);
    svg_lint::lint_tree_with_options(
        bytes,
        &tree,
        LintOptions {
            profile,
            native: None,
            edition: None,
        },
        None,
    )
}

/// Render the human-readable, file-grouped report.
fn write_text(
    out: &mut impl Write,
    reports: &[FileReport],
    threshold: SeverityArg,
    quiet: bool,
) -> io::Result<()> {
    let mut shown_total = 0usize;
    for report in reports {
        let visible: Vec<&SvgDiagnostic> = report
            .diagnostics
            .iter()
            .filter(|diag| !quiet || threshold.admits(diag.severity))
            .collect();
        if visible.is_empty() {
            continue;
        }
        writeln!(out, "{}", report.label)?;
        for diag in visible {
            // Display 1-based line:col; the library reports 0-based rows
            // and byte columns.
            writeln!(
                out,
                "  {}:{} {} [{}] {}",
                diag.start_row + 1,
                diag.start_col + 1,
                severity_label(diag.severity),
                diag.code.as_str(),
                diag.message,
            )?;
            shown_total += 1;
        }
    }

    if shown_total == 0 {
        writeln!(out, "No diagnostics.")?;
    }
    Ok(())
}

/// Render the machine-readable JSON report: an array of `{ file,
/// diagnostics: [...] }` objects.
fn write_json(
    out: &mut impl Write,
    reports: &[FileReport],
    threshold: SeverityArg,
    quiet: bool,
) -> io::Result<()> {
    let files: Vec<serde_json::Value> = reports
        .iter()
        .map(|report| {
            let diagnostics: Vec<serde_json::Value> = report
                .diagnostics
                .iter()
                .filter(|diag| !quiet || threshold.admits(diag.severity))
                .map(diagnostic_to_json)
                .collect();
            serde_json::json!({
                "file": report.label,
                "diagnostics": diagnostics,
            })
        })
        .collect();

    let rendered = serde_json::to_string_pretty(&files).map_err(std::io::Error::other)?;
    out.write_all(rendered.as_bytes())?;
    out.write_all(b"\n")
}

/// Serialise a single diagnostic into a stable JSON shape. 0-based rows
/// and byte columns are preserved as-is for editor/CI tooling; a
/// 1-based `line`/`column` pair is added for convenience.
fn diagnostic_to_json(diag: &SvgDiagnostic) -> serde_json::Value {
    serde_json::json!({
        "code": diag.code.as_str(),
        "severity": severity_label(diag.severity),
        "message": diag.message,
        "line": diag.start_row + 1,
        "column": diag.start_col + 1,
        "range": {
            "startRow": diag.start_row,
            "startCol": diag.start_col,
            "endRow": diag.end_row,
            "endCol": diag.end_col,
            "startByte": diag.byte_range.start,
            "endByte": diag.byte_range.end,
        },
    })
}

/// Canonical profile ids plus their aliases, for error help text.
fn known_profiles() -> Vec<String> {
    let mut out = Vec::new();
    for snapshot in svg_data::spec_snapshots() {
        let mut line = String::new();
        let _ = write!(line, "{}", snapshot.as_str());
        let aliases = svg_data::snapshot_metadata(*snapshot).aliases;
        if !aliases.is_empty() {
            let _ = write!(line, " ({})", aliases.join(", "));
        }
        out.push(line);
    }
    out
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(message) => {
            eprintln!("svg-lint: {message}");
            ExitCode::from(EXIT_USAGE)
        }
    }
}
