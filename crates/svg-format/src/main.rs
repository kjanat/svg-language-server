//! Command-line entrypoint for the `svg-format` formatter.

use std::{
    fs,
    io::{self, Read, Write},
    path::PathBuf,
    process::ExitCode,
};

use clap::{Args, Parser};
use svg_format::{
    AttributeLayout, AttributeSort, BlankLines, FormatOptions, QuoteStyle, TextContentMode,
    WrappedAttributeIndent, format_with_options,
};

#[derive(Debug, Parser)]
#[command(
    name = "svg-format",
    version,
    about = "Structural formatter for SVG documents",
    long_about = "Deterministic, opinionated formatter for SVG documents.\n\
Reads SVG from FILE or stdin and writes the formatted result to stdout.\n\
Defaults mirror the W3 SVG reference style: 2-space indent, canonical\n\
attribute order, multi-line wrapping aligned under the tag name."
)]
struct Cli {
    /// SVG file to format. Reads stdin when omitted.
    #[arg(value_name = "FILE", conflicts_with = "stdin")]
    path: Option<PathBuf>,

    /// I/O mode selectors (in-place, check, stdin).
    #[command(flatten)]
    io: IoArgs,

    /// Spaces per indentation level (used when not in tab mode).
    #[arg(long, default_value_t = FormatOptions::default().indent_width)]
    indent_width: usize,

    /// Indent character selector (tabs vs spaces).
    #[command(flatten)]
    indent_style: IndentStyleArgs,

    /// Maximum inline tag width before switching to multi-line layout.
    #[arg(long, default_value_t = FormatOptions::default().max_inline_tag_width)]
    max_inline_tag_width: usize,

    /// Attribute ordering mode.
    #[arg(long, value_enum, default_value_t = FormatOptions::default().attribute_sort)]
    attribute_sort: AttributeSort,

    /// Attribute wrapping mode.
    #[arg(long, value_enum, default_value_t = FormatOptions::default().attribute_layout)]
    attribute_layout: AttributeLayout,

    /// Maximum attributes emitted per wrapped line.
    #[arg(long, default_value_t = FormatOptions::default().attributes_per_line)]
    attributes_per_line: usize,

    /// Self-closing tag spacing toggle.
    #[command(flatten)]
    self_close: SelfCloseArgs,

    /// Preferred quote style for attribute values.
    #[arg(long, value_enum, default_value_t = FormatOptions::default().quote_style)]
    quote_style: QuoteStyle,

    /// Indentation style for wrapped attribute lines.
    #[arg(long, value_enum, default_value_t = FormatOptions::default().wrapped_attribute_indent)]
    wrapped_attribute_indent: WrappedAttributeIndent,

    /// How whitespace inside text nodes is handled.
    #[arg(long, value_enum, default_value_t = FormatOptions::default().text_content)]
    text_content: TextContentMode,

    /// How blank lines between sibling elements are handled.
    #[arg(long, value_enum, default_value_t = FormatOptions::default().blank_lines)]
    blank_lines: BlankLines,

    /// Comment prefix that triggers ignore directives (repeatable).
    #[arg(long = "ignore-prefix", value_name = "PREFIX")]
    ignore_prefixes: Vec<String>,
}

/// Input/output mode flags: rewrite in place, check-only, or read stdin.
#[derive(Args, Debug)]
struct IoArgs {
    /// Rewrite FILE in place instead of writing to stdout.
    #[arg(short = 'i', long, requires = "path")]
    in_place: bool,

    /// Exit non-zero if FILE is not already formatted; do not write.
    #[arg(long)]
    check: bool,

    /// Read SVG from stdin. Conflicts with FILE.
    #[arg(long, conflicts_with = "path")]
    stdin: bool,
}

/// Mutually exclusive indent-character selector. Unset → use library default.
#[derive(Args, Debug)]
struct IndentStyleArgs {
    /// Use a tab character per indentation level.
    #[arg(long, conflicts_with = "use_spaces")]
    use_tabs: bool,

    /// Use `--indent-width` spaces per indentation level.
    #[arg(long, conflicts_with = "use_tabs")]
    use_spaces: bool,
}

/// Mutually exclusive `/>` spacing toggle. Unset → use library default.
#[derive(Args, Debug)]
struct SelfCloseArgs {
    /// Emit a space before `/>` in self-closing tags.
    #[arg(long, conflicts_with = "no_space_before_self_close")]
    space_before_self_close: bool,

    /// Omit the space before `/>` in self-closing tags.
    #[arg(long, conflicts_with = "space_before_self_close")]
    no_space_before_self_close: bool,
}

fn run() -> Result<ExitCode, String> {
    let cli = Cli::parse();
    let defaults = FormatOptions::default();
    let read_stdin = cli.io.stdin || cli.path.is_none();

    if cli.io.in_place && read_stdin {
        return Err("--in-place requires FILE input".to_string());
    }
    if cli.indent_width == 0 {
        return Err("--indent-width must be greater than 0".to_string());
    }
    if cli.max_inline_tag_width == 0 {
        return Err("--max-inline-tag-width must be greater than 0".to_string());
    }
    if cli.attributes_per_line == 0 {
        return Err("--attributes-per-line must be greater than 0".to_string());
    }

    let options = FormatOptions {
        indent_width: cli.indent_width,
        insert_spaces: if cli.indent_style.use_spaces {
            true
        } else if cli.indent_style.use_tabs {
            false
        } else {
            defaults.insert_spaces
        },
        max_inline_tag_width: cli.max_inline_tag_width,
        attribute_sort: cli.attribute_sort,
        attribute_layout: cli.attribute_layout,
        attributes_per_line: cli.attributes_per_line,
        space_before_self_close: if cli.self_close.no_space_before_self_close {
            false
        } else if cli.self_close.space_before_self_close {
            true
        } else {
            defaults.space_before_self_close
        },
        quote_style: cli.quote_style,
        wrapped_attribute_indent: cli.wrapped_attribute_indent,
        text_content: cli.text_content,
        blank_lines: cli.blank_lines,
        ignore_prefixes: merge_ignore_prefixes(defaults.ignore_prefixes, cli.ignore_prefixes),
    };

    let input = match (read_stdin, cli.path.as_ref()) {
        (true, _) | (_, None) => {
            let mut input = String::new();
            io::stdin()
                .read_to_string(&mut input)
                .map_err(|err| format!("failed reading stdin: {err}"))?;
            input
        }
        (false, Some(path)) => fs::read_to_string(path)
            .map_err(|err| format!("failed reading '{}': {err}", path.display()))?,
    };

    let formatted = format_with_options(&input, options);
    let changed = formatted != input;

    if cli.io.check {
        if changed {
            if let Some(path) = &cli.path {
                eprintln!("would reformat {}", path.display());
            } else {
                eprintln!("would reformat <stdin>");
            }
            return Ok(ExitCode::from(1));
        }
        return Ok(ExitCode::SUCCESS);
    }

    if cli.io.in_place {
        if changed {
            let Some(path) = cli.path.as_ref() else {
                unreachable!("--in-place requires FILE input");
            };
            fs::write(path, formatted)
                .map_err(|err| format!("failed writing '{}': {err}", path.display()))?;
        }
        return Ok(ExitCode::SUCCESS);
    }

    let mut stdout = io::stdout().lock();
    stdout
        .write_all(formatted.as_bytes())
        .map_err(|err| format!("failed writing stdout: {err}"))?;
    Ok(ExitCode::SUCCESS)
}

fn merge_ignore_prefixes(mut defaults: Vec<String>, extra: Vec<String>) -> Vec<String> {
    for prefix in extra {
        if !defaults.iter().any(|existing| existing == &prefix) {
            defaults.push(prefix);
        }
    }
    defaults
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(message) => {
            eprintln!("svg-format: {message}");
            ExitCode::from(2)
        }
    }
}
