use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, ValueEnum};
use svg_format::{
    AttributeLayout, AttributeSort, FormatOptions, QuoteStyle, WrappedAttributeIndent,
    format_with_options,
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
        text_content: defaults.text_content,
        blank_lines: defaults.blank_lines,
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
            let path = cli.path.as_ref().expect("path checked above");
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

#[cfg(test)]
mod debug_tests {
    use tree_sitter::Parser;

    #[test]
    fn debug_tree_structure() {
        let source = r#"<svg><style>
  .a { fill: red; }
</style><script>
  console.log("hello");
</script></svg>"#;

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_svg::LANGUAGE.into())
            .unwrap();

        if let Some(tree) = parser.parse(source.as_bytes(), None) {
            fn print_tree(node: tree_sitter::Node, source: &[u8], depth: usize) {
                let indent = "  ".repeat(depth);
                let text = std::str::from_utf8(&source[node.byte_range()]).unwrap_or("?");
                let short = if text.len() > 30 { &text[..30] } else { text };
                eprintln!("{}kind={} text={:?}", indent, node.kind(), short);

                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    print_tree(child, source, depth + 1);
                }
            }
            print_tree(tree.root_node(), source.as_bytes(), 0);
        }
    }
}
