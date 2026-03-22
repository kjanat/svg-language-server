use anyhow::{Result, anyhow};
use dprint_core::configuration::{
    ConfigKeyMap, ConfigurationDiagnostic, GlobalConfiguration, NewLineKind,
    ParseConfigurationError, RawNewLineKind, get_unknown_property_diagnostics, get_value,
    resolve_new_line_kind,
};
use dprint_core::plugins::{
    CheckConfigUpdatesMessage, ConfigChange, FileMatchingInfo, FormatResult, PluginInfo,
    PluginResolveConfigurationResult, SyncFormatRequest, SyncHostFormatRequest, SyncPluginHandler,
};
use serde::Serialize;
use svg_format::{
    AttributeLayout, AttributeSort, FormatOptions, QuoteStyle, WrappedAttributeIndent,
    format_with_options,
};

#[derive(Default)]
pub struct SvgWasmPluginHandler;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AttributeSortConfig {
    None,
    Canonical,
    Alphabetical,
}
dprint_core::generate_str_to_from![
    AttributeSortConfig,
    [None, "none"],
    [Canonical, "canonical"],
    [Alphabetical, "alphabetical"]
];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AttributeLayoutConfig {
    Auto,
    SingleLine,
    MultiLine,
}
dprint_core::generate_str_to_from![
    AttributeLayoutConfig,
    [Auto, "auto"],
    [SingleLine, "single-line"],
    [MultiLine, "multi-line"]
];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum QuoteStyleConfig {
    Preserve,
    Double,
    Single,
}
dprint_core::generate_str_to_from![
    QuoteStyleConfig,
    [Preserve, "preserve"],
    [Double, "double"],
    [Single, "single"]
];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WrappedAttributeIndentConfig {
    OneLevel,
    AlignToTagName,
}
dprint_core::generate_str_to_from![
    WrappedAttributeIndentConfig,
    [OneLevel, "one-level"],
    [AlignToTagName, "align-to-tag-name"]
];

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Configuration {
    pub max_inline_tag_width: u32,
    pub use_tabs: bool,
    pub indent_width: u8,
    pub attribute_sort: AttributeSortConfig,
    pub attribute_layout: AttributeLayoutConfig,
    pub attributes_per_line: u32,
    pub space_before_self_close: bool,
    pub quote_style: QuoteStyleConfig,
    pub wrapped_attribute_indent: WrappedAttributeIndentConfig,
    pub new_line_kind: NewLineKind,
}

impl SyncPluginHandler<Configuration> for SvgWasmPluginHandler {
    fn plugin_info(&mut self) -> PluginInfo {
        PluginInfo {
            name: env!("CARGO_PKG_NAME").to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            config_key: "svg".to_string(),
            help_url:
                "https://github.com/kjanat/svg-language-server/tree/master/crates/dprint-plugin-svg"
                    .to_string(),
            config_schema_url: "".to_string(),
            update_url: None,
        }
    }

    fn license_text(&mut self) -> String {
        include_str!("../../../LICENSE").to_string()
    }

    fn resolve_config(
        &mut self,
        mut config: ConfigKeyMap,
        global_config: &GlobalConfiguration,
    ) -> PluginResolveConfigurationResult<Configuration> {
        let mut diagnostics = Vec::<ConfigurationDiagnostic>::new();

        let line_width = get_value(
            &mut config,
            "lineWidth",
            global_config.line_width.unwrap_or(100),
            &mut diagnostics,
        );
        let max_inline_tag_width = get_value(
            &mut config,
            "maxInlineTagWidth",
            line_width,
            &mut diagnostics,
        );
        let use_tabs = get_value(
            &mut config,
            "useTabs",
            global_config.use_tabs.unwrap_or(true),
            &mut diagnostics,
        );
        let indent_width = get_value(
            &mut config,
            "indentWidth",
            global_config.indent_width.unwrap_or(2),
            &mut diagnostics,
        );
        let attribute_sort = get_value(
            &mut config,
            "attributeSort",
            AttributeSortConfig::Canonical,
            &mut diagnostics,
        );
        let attribute_layout = get_value(
            &mut config,
            "attributeLayout",
            AttributeLayoutConfig::Auto,
            &mut diagnostics,
        );
        let mut attributes_per_line =
            get_value(&mut config, "attributesPerLine", 1_u32, &mut diagnostics);
        let space_before_self_close =
            get_value(&mut config, "spaceBeforeSelfClose", true, &mut diagnostics);
        let quote_style = get_value(
            &mut config,
            "quoteStyle",
            QuoteStyleConfig::Preserve,
            &mut diagnostics,
        );
        let wrapped_attribute_indent = get_value(
            &mut config,
            "wrappedAttributeIndent",
            WrappedAttributeIndentConfig::OneLevel,
            &mut diagnostics,
        );
        let raw_new_line_kind = get_value(
            &mut config,
            "newLineKind",
            RawNewLineKind::Auto,
            &mut diagnostics,
        );
        let new_line_kind = match raw_new_line_kind {
            RawNewLineKind::Auto => NewLineKind::Auto,
            RawNewLineKind::LineFeed => NewLineKind::LineFeed,
            RawNewLineKind::CarriageReturnLineFeed => NewLineKind::CarriageReturnLineFeed,
            RawNewLineKind::System => {
                if cfg!(windows) {
                    NewLineKind::CarriageReturnLineFeed
                } else {
                    NewLineKind::LineFeed
                }
            }
        };

        if attributes_per_line == 0 {
            diagnostics.push(ConfigurationDiagnostic {
                property_name: "attributesPerLine".to_string(),
                message: "Expected a value greater than 0.".to_string(),
            });
            attributes_per_line = 1;
        }

        diagnostics.extend(get_unknown_property_diagnostics(config));

        PluginResolveConfigurationResult {
            file_matching: FileMatchingInfo {
                file_extensions: vec!["svg".to_string(), "svgz".to_string()],
                file_names: Vec::new(),
            },
            diagnostics,
            config: Configuration {
                max_inline_tag_width,
                use_tabs,
                indent_width,
                attribute_sort,
                attribute_layout,
                attributes_per_line,
                space_before_self_close,
                quote_style,
                wrapped_attribute_indent,
                new_line_kind,
            },
        }
    }

    fn check_config_updates(
        &self,
        _message: CheckConfigUpdatesMessage,
    ) -> Result<Vec<ConfigChange>> {
        Ok(Vec::new())
    }

    fn format(
        &mut self,
        request: SyncFormatRequest<Configuration>,
        _format_with_host: impl FnMut(SyncHostFormatRequest) -> FormatResult,
    ) -> FormatResult {
        if request.range.is_some() || request.token.is_cancelled() {
            return Ok(None);
        }

        let source = std::str::from_utf8(&request.file_bytes).map_err(|err| {
            anyhow!(
                "Could not decode '{}' as UTF-8: {err}",
                request.file_path.display()
            )
        })?;

        let options = FormatOptions {
            indent_width: request.config.indent_width as usize,
            insert_spaces: !request.config.use_tabs,
            max_inline_tag_width: request.config.max_inline_tag_width as usize,
            attribute_sort: map_attribute_sort(request.config.attribute_sort),
            attribute_layout: map_attribute_layout(request.config.attribute_layout),
            attributes_per_line: request.config.attributes_per_line as usize,
            space_before_self_close: request.config.space_before_self_close,
            quote_style: map_quote_style(request.config.quote_style),
            wrapped_attribute_indent: map_wrapped_attribute_indent(
                request.config.wrapped_attribute_indent,
            ),
        };

        let mut formatted = format_with_options(source, options);
        let newline = resolve_new_line_kind(source, request.config.new_line_kind);
        if newline != "\n" {
            formatted = formatted.replace('\n', newline);
        }

        if formatted.as_bytes() == request.file_bytes.as_slice() {
            Ok(None)
        } else {
            Ok(Some(formatted.into_bytes()))
        }
    }
}

fn map_attribute_sort(value: AttributeSortConfig) -> AttributeSort {
    match value {
        AttributeSortConfig::None => AttributeSort::None,
        AttributeSortConfig::Canonical => AttributeSort::Canonical,
        AttributeSortConfig::Alphabetical => AttributeSort::Alphabetical,
    }
}

fn map_attribute_layout(value: AttributeLayoutConfig) -> AttributeLayout {
    match value {
        AttributeLayoutConfig::Auto => AttributeLayout::Auto,
        AttributeLayoutConfig::SingleLine => AttributeLayout::SingleLine,
        AttributeLayoutConfig::MultiLine => AttributeLayout::MultiLine,
    }
}

fn map_quote_style(value: QuoteStyleConfig) -> QuoteStyle {
    match value {
        QuoteStyleConfig::Preserve => QuoteStyle::Preserve,
        QuoteStyleConfig::Double => QuoteStyle::Double,
        QuoteStyleConfig::Single => QuoteStyle::Single,
    }
}

fn map_wrapped_attribute_indent(value: WrappedAttributeIndentConfig) -> WrappedAttributeIndent {
    match value {
        WrappedAttributeIndentConfig::OneLevel => WrappedAttributeIndent::OneLevel,
        WrappedAttributeIndentConfig::AlignToTagName => WrappedAttributeIndent::AlignToTagName,
    }
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
dprint_core::generate_plugin_code!(
    SvgWasmPluginHandler,
    SvgWasmPluginHandler,
    Configuration
);
