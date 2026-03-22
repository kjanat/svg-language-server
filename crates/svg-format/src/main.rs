use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use svg_format::{
    AttributeLayout, AttributeSort, FormatOptions, QuoteStyle, WrappedAttributeIndent,
    format_with_options,
};

#[derive(Debug)]
struct CliConfig {
    options: FormatOptions,
    check: bool,
    in_place: bool,
    read_stdin: bool,
    path: Option<PathBuf>,
}

fn print_help() {
    println!(
        "svg-format 0.1.0

Usage:
  svg-format [OPTIONS] [FILE]
  svg-format --stdin [OPTIONS]

Options:
  -h, --help                              Show this help
  -i, --in-place                          Write formatted output back to FILE
      --check                             Exit 1 when formatting would change output
      --stdin                             Read input from stdin
      --indent-width <N>                  Indent width when using spaces (default: 2)
      --use-tabs                          Use tabs for indentation (default)
      --use-spaces                        Use spaces for indentation
      --max-inline-tag-width <N>          Inline width threshold before wrapping (default: 100)
      --attribute-sort <MODE>             none|canonical|alphabetical (default: canonical)
      --attribute-layout <MODE>           auto|single-line|multi-line (default: auto)
      --attributes-per-line <N>           Wrapped attributes per line (default: 1)
      --space-before-self-close           Emit space before '/>' (default)
      --no-space-before-self-close        Omit space before '/>'
      --quote-style <MODE>                preserve|double|single (default: preserve)
      --wrapped-attribute-indent <MODE>   one-level|align-to-tag-name (default: one-level)
"
    );
}

fn parse_attribute_sort(value: &str) -> Result<AttributeSort, String> {
    match value {
        "none" => Ok(AttributeSort::None),
        "canonical" => Ok(AttributeSort::Canonical),
        "alphabetical" => Ok(AttributeSort::Alphabetical),
        _ => Err(format!(
            "invalid --attribute-sort value '{value}' (expected none|canonical|alphabetical)"
        )),
    }
}

fn parse_attribute_layout(value: &str) -> Result<AttributeLayout, String> {
    match value {
        "auto" => Ok(AttributeLayout::Auto),
        "single-line" | "single_line" | "single" => Ok(AttributeLayout::SingleLine),
        "multi-line" | "multi_line" | "multi" => Ok(AttributeLayout::MultiLine),
        _ => Err(format!(
            "invalid --attribute-layout value '{value}' (expected auto|single-line|multi-line)"
        )),
    }
}

fn parse_quote_style(value: &str) -> Result<QuoteStyle, String> {
    match value {
        "preserve" => Ok(QuoteStyle::Preserve),
        "double" => Ok(QuoteStyle::Double),
        "single" => Ok(QuoteStyle::Single),
        _ => Err(format!(
            "invalid --quote-style value '{value}' (expected preserve|double|single)"
        )),
    }
}

fn parse_wrapped_attribute_indent(value: &str) -> Result<WrappedAttributeIndent, String> {
    match value {
        "one-level" | "one_level" | "indent" => Ok(WrappedAttributeIndent::OneLevel),
        "align-to-tag-name" | "align_to_tag_name" | "align" => {
            Ok(WrappedAttributeIndent::AlignToTagName)
        }
        _ => Err(format!(
            "invalid --wrapped-attribute-indent value '{value}' (expected one-level|align-to-tag-name)"
        )),
    }
}

fn parse_usize_flag(flag: &str, value: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .map_err(|_| format!("invalid value for {flag}: '{value}'"))
}

fn parse_args() -> Result<Option<CliConfig>, String> {
    let mut config = CliConfig {
        options: FormatOptions::default(),
        check: false,
        in_place: false,
        read_stdin: false,
        path: None,
    };

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                return Ok(None);
            }
            "-i" | "--in-place" => {
                config.in_place = true;
            }
            "--check" => {
                config.check = true;
            }
            "--stdin" => {
                config.read_stdin = true;
            }
            "--use-tabs" => {
                config.options.insert_spaces = false;
            }
            "--use-spaces" => {
                config.options.insert_spaces = true;
            }
            "--space-before-self-close" => {
                config.options.space_before_self_close = true;
            }
            "--no-space-before-self-close" => {
                config.options.space_before_self_close = false;
            }
            "--indent-width" => {
                let Some(value) = args.next() else {
                    return Err("--indent-width requires a value".to_string());
                };
                config.options.indent_width = parse_usize_flag("--indent-width", &value)?;
            }
            "--max-inline-tag-width" => {
                let Some(value) = args.next() else {
                    return Err("--max-inline-tag-width requires a value".to_string());
                };
                config.options.max_inline_tag_width =
                    parse_usize_flag("--max-inline-tag-width", &value)?;
            }
            "--attribute-sort" => {
                let Some(value) = args.next() else {
                    return Err("--attribute-sort requires a value".to_string());
                };
                config.options.attribute_sort = parse_attribute_sort(&value)?;
            }
            "--attribute-layout" => {
                let Some(value) = args.next() else {
                    return Err("--attribute-layout requires a value".to_string());
                };
                config.options.attribute_layout = parse_attribute_layout(&value)?;
            }
            "--attributes-per-line" => {
                let Some(value) = args.next() else {
                    return Err("--attributes-per-line requires a value".to_string());
                };
                config.options.attributes_per_line =
                    parse_usize_flag("--attributes-per-line", &value)?;
            }
            "--quote-style" => {
                let Some(value) = args.next() else {
                    return Err("--quote-style requires a value".to_string());
                };
                config.options.quote_style = parse_quote_style(&value)?;
            }
            "--wrapped-attribute-indent" => {
                let Some(value) = args.next() else {
                    return Err("--wrapped-attribute-indent requires a value".to_string());
                };
                config.options.wrapped_attribute_indent = parse_wrapped_attribute_indent(&value)?;
            }
            _ if arg.starts_with('-') => {
                return Err(format!("unknown option: {arg}"));
            }
            _ => {
                if config.path.is_some() {
                    return Err("multiple input files provided; only one FILE is supported".into());
                }
                config.path = Some(PathBuf::from(arg));
            }
        }
    }

    if config.read_stdin && config.path.is_some() {
        return Err("cannot use --stdin and FILE together".to_string());
    }
    if config.in_place && config.path.is_none() {
        return Err("--in-place requires FILE input".to_string());
    }
    if config.in_place && config.read_stdin {
        return Err("--in-place cannot be used with --stdin".to_string());
    }

    Ok(Some(config))
}

fn run() -> Result<ExitCode, String> {
    let Some(config) = parse_args()? else {
        return Ok(ExitCode::SUCCESS);
    };

    let input = match (config.read_stdin, config.path.as_ref()) {
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

    let formatted = format_with_options(&input, config.options);
    let changed = formatted != input;

    if config.check {
        if changed {
            if let Some(path) = &config.path {
                eprintln!("would reformat {}", path.display());
            } else {
                eprintln!("would reformat <stdin>");
            }
            return Ok(ExitCode::from(1));
        }
        return Ok(ExitCode::SUCCESS);
    }

    if config.in_place {
        if changed {
            let path = config.path.as_ref().expect("path checked above");
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
            eprintln!("Use --help for usage.");
            ExitCode::from(2)
        }
    }
}
