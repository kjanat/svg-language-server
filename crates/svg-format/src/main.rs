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
    about = "Structural formatter for SVG documents"
)]
struct Cli {
    #[arg(value_name = "FILE", conflicts_with = "stdin")]
    path: Option<PathBuf>,

    #[command(flatten)]
    io: IoArgs,

    #[arg(long, default_value_t = 2)]
    indent_width: usize,

    #[command(flatten)]
    indent_style: IndentStyleArgs,

    #[arg(long, default_value_t = 100)]
    max_inline_tag_width: usize,

    #[arg(long, value_enum, default_value_t = AttributeSort::Canonical)]
    attribute_sort: AttributeSort,

    #[arg(long, value_enum, default_value_t = AttributeLayout::Auto)]
    attribute_layout: AttributeLayout,

    #[arg(long, default_value_t = 1)]
    attributes_per_line: usize,

    #[command(flatten)]
    self_close: SelfCloseArgs,

    #[arg(long, value_enum, default_value_t = QuoteStyle::Preserve)]
    quote_style: QuoteStyle,

    #[arg(long, value_enum, default_value_t = WrappedAttributeIndent::OneLevel)]
    wrapped_attribute_indent: WrappedAttributeIndent,

    #[arg(long, value_enum, default_value_t = TextContentMode::Maintain)]
    text_content: TextContentMode,

    #[arg(long, value_enum, default_value_t = BlankLines::Truncate)]
    blank_lines: BlankLines,
}

#[derive(Args, Debug)]
struct IoArgs {
    #[arg(short = 'i', long, requires = "path")]
    in_place: bool,

    #[arg(long)]
    check: bool,

    #[arg(long, conflicts_with = "path")]
    stdin: bool,
}

#[derive(Args, Debug)]
struct IndentStyleArgs {
    #[arg(long, conflicts_with = "use_spaces")]
    use_tabs: bool,

    #[arg(long, conflicts_with = "use_tabs")]
    use_spaces: bool,
}

#[derive(Args, Debug)]
struct SelfCloseArgs {
    #[arg(long, conflicts_with = "no_space_before_self_close")]
    space_before_self_close: bool,

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
        ignore_prefixes: defaults.ignore_prefixes,
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
                return Err("--in-place requires FILE input".to_string());
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

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(message) => {
            eprintln!("svg-format: {message}");
            ExitCode::from(2)
        }
    }
}
