//! Command-line entrypoint for the `svg-format` formatter.

use std::{
    fs,
    io::{self, Read, Write},
    path::PathBuf,
    process::ExitCode,
};

use clap::{Parser, ValueEnum};
use svg_format::{
    AttributeLayout, AttributeSort, BlankLines, FormatOptions, QuoteStyle, TextContentMode,
    WrappedAttributeIndent, format_with_options,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
enum AttributeSortArg {
    None,
    Canonical,
    Alphabetical,
}

impl From<AttributeSortArg> for AttributeSort {
    fn from(value: AttributeSortArg) -> Self {
        match value {
            AttributeSortArg::None => AttributeSort::None,
            AttributeSortArg::Canonical => AttributeSort::Canonical,
            AttributeSortArg::Alphabetical => AttributeSort::Alphabetical,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
enum AttributeLayoutArg {
    Auto,
    SingleLine,
    MultiLine,
}

impl From<AttributeLayoutArg> for AttributeLayout {
    fn from(value: AttributeLayoutArg) -> Self {
        match value {
            AttributeLayoutArg::Auto => AttributeLayout::Auto,
            AttributeLayoutArg::SingleLine => AttributeLayout::SingleLine,
            AttributeLayoutArg::MultiLine => AttributeLayout::MultiLine,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
enum QuoteStyleArg {
    Preserve,
    Double,
    Single,
}

impl From<QuoteStyleArg> for QuoteStyle {
    fn from(value: QuoteStyleArg) -> Self {
        match value {
            QuoteStyleArg::Preserve => QuoteStyle::Preserve,
            QuoteStyleArg::Double => QuoteStyle::Double,
            QuoteStyleArg::Single => QuoteStyle::Single,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
enum WrappedAttributeIndentArg {
    OneLevel,
    AlignToTagName,
}

impl From<WrappedAttributeIndentArg> for WrappedAttributeIndent {
    fn from(value: WrappedAttributeIndentArg) -> Self {
        match value {
            WrappedAttributeIndentArg::OneLevel => WrappedAttributeIndent::OneLevel,
            WrappedAttributeIndentArg::AlignToTagName => WrappedAttributeIndent::AlignToTagName,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
enum TextContentArg {
    Collapse,
    Maintain,
    Prettify,
}

impl From<TextContentArg> for TextContentMode {
    fn from(value: TextContentArg) -> Self {
        match value {
            TextContentArg::Collapse => TextContentMode::Collapse,
            TextContentArg::Maintain => TextContentMode::Maintain,
            TextContentArg::Prettify => TextContentMode::Prettify,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
enum BlankLinesArg {
    Remove,
    Preserve,
    Truncate,
    Insert,
}

impl From<BlankLinesArg> for BlankLines {
    fn from(value: BlankLinesArg) -> Self {
        match value {
            BlankLinesArg::Remove => BlankLines::Remove,
            BlankLinesArg::Preserve => BlankLines::Preserve,
            BlankLinesArg::Truncate => BlankLines::Truncate,
            BlankLinesArg::Insert => BlankLines::Insert,
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "svg-format",
    version,
    about = "Structural formatter for SVG documents"
)]
struct Cli {
    #[arg(value_name = "FILE", conflicts_with = "stdin")]
    path: Option<PathBuf>,

    #[arg(short = 'i', long, requires = "path")]
    in_place: bool,

    #[arg(long)]
    check: bool,

    #[arg(long, conflicts_with = "path")]
    stdin: bool,

    #[arg(long, default_value_t = 2)]
    indent_width: usize,

    #[arg(long, conflicts_with = "use_spaces")]
    use_tabs: bool,

    #[arg(long, conflicts_with = "use_tabs")]
    use_spaces: bool,

    #[arg(long, default_value_t = 100)]
    max_inline_tag_width: usize,

    #[arg(long, value_enum, default_value_t = AttributeSortArg::Canonical)]
    attribute_sort: AttributeSortArg,

    #[arg(long, value_enum, default_value_t = AttributeLayoutArg::Auto)]
    attribute_layout: AttributeLayoutArg,

    #[arg(long, default_value_t = 1)]
    attributes_per_line: usize,

    #[arg(long, conflicts_with = "no_space_before_self_close")]
    space_before_self_close: bool,

    #[arg(long, conflicts_with = "space_before_self_close")]
    no_space_before_self_close: bool,

    #[arg(long, value_enum, default_value_t = QuoteStyleArg::Preserve)]
    quote_style: QuoteStyleArg,

    #[arg(long, value_enum, default_value_t = WrappedAttributeIndentArg::OneLevel)]
    wrapped_attribute_indent: WrappedAttributeIndentArg,

    #[arg(long, value_enum, default_value_t = TextContentArg::Maintain)]
    text_content: TextContentArg,

    #[arg(long, value_enum, default_value_t = BlankLinesArg::Truncate)]
    blank_lines: BlankLinesArg,
}

fn run() -> Result<ExitCode, String> {
    let cli = Cli::parse();
    let defaults = FormatOptions::default();
    let read_stdin = cli.stdin || cli.path.is_none();

    if cli.in_place && read_stdin {
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
        insert_spaces: if cli.use_spaces {
            true
        } else if cli.use_tabs {
            false
        } else {
            defaults.insert_spaces
        },
        max_inline_tag_width: cli.max_inline_tag_width,
        attribute_sort: cli.attribute_sort.into(),
        attribute_layout: cli.attribute_layout.into(),
        attributes_per_line: cli.attributes_per_line,
        space_before_self_close: if cli.no_space_before_self_close {
            false
        } else if cli.space_before_self_close {
            true
        } else {
            defaults.space_before_self_close
        },
        quote_style: cli.quote_style.into(),
        wrapped_attribute_indent: cli.wrapped_attribute_indent.into(),
        text_content: cli.text_content.into(),
        blank_lines: cli.blank_lines.into(),
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

    if cli.check {
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

    if cli.in_place {
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
