use std::{
    fs,
    sync::{Arc, OnceLock},
    time::Duration,
};

use tower_lsp_server::ls_types::{GotoDefinitionResponse, Location, Uri};
use url::Url;

use crate::StylesheetCache;

#[derive(Clone)]
pub struct CachedStylesheet {
    pub uri: Uri,
    pub source: String,
    pub class_definitions: Vec<svg_references::NamedSpan>,
    pub custom_property_definitions: Vec<svg_references::NamedSpan>,
}

#[derive(Clone)]
pub struct ClassDefinitionHover {
    pub uri: Uri,
    pub source: String,
    pub definition: svg_references::NamedSpan,
}

impl ClassDefinitionHover {
    pub const fn new(uri: Uri, source: String, definition: svg_references::NamedSpan) -> Self {
        Self {
            uri,
            source,
            definition,
        }
    }
}

#[derive(Clone)]
pub struct CustomPropertyDefinitionHover {
    pub uri: Uri,
    pub source: String,
    pub definition: svg_references::NamedSpan,
}

impl CustomPropertyDefinitionHover {
    pub const fn new(uri: Uri, source: String, definition: svg_references::NamedSpan) -> Self {
        Self {
            uri,
            source,
            definition,
        }
    }
}

pub fn class_definition_hovers_from_stylesheet(
    uri: &Uri,
    source: &str,
    target_class: &str,
) -> Vec<ClassDefinitionHover> {
    svg_references::collect_class_definitions_from_stylesheet(source, 0, 0)
        .into_iter()
        .filter(|definition| definition.name == target_class)
        .map(|definition| ClassDefinitionHover::new(uri.clone(), source.to_owned(), definition))
        .collect()
}

pub fn custom_property_definition_hovers_from_stylesheet(
    uri: &Uri,
    source: &str,
    target_property: &str,
) -> Vec<CustomPropertyDefinitionHover> {
    svg_references::collect_custom_property_definitions_from_stylesheet(source, 0, 0)
        .into_iter()
        .filter(|definition| definition.name == target_property)
        .map(|definition| {
            CustomPropertyDefinitionHover::new(uri.clone(), source.to_owned(), definition)
        })
        .collect()
}

pub fn definition_response_from_locations(
    mut locations: Vec<Location>,
) -> Option<GotoDefinitionResponse> {
    if locations.is_empty() {
        return None;
    }
    if locations.len() == 1 {
        return Some(GotoDefinitionResponse::Scalar(locations.remove(0)));
    }
    Some(GotoDefinitionResponse::Array(locations))
}

fn parse_stylesheet(uri: Uri, source: String) -> CachedStylesheet {
    let class_definitions =
        svg_references::collect_class_definitions_from_stylesheet(&source, 0, 0);
    let custom_property_definitions =
        svg_references::collect_custom_property_definitions_from_stylesheet(&source, 0, 0);
    CachedStylesheet {
        uri,
        source,
        class_definitions,
        custom_property_definitions,
    }
}

pub fn resolve_stylesheet_url(base_uri: &Uri, href: &str) -> Option<Url> {
    if let Ok(url) = Url::parse(href) {
        return Some(url);
    }

    let base = Url::parse(base_uri.as_str())
        .map_err(
            |err| tracing::warn!(uri = base_uri.as_str(), error = %err, "malformed document URI"),
        )
        .ok()?;
    base.join(href)
        .map_err(|err| tracing::warn!(href, error = %err, "failed to resolve stylesheet URL"))
        .ok()
}

pub fn resolve_file_stylesheet(url: &Url) -> Option<CachedStylesheet> {
    let path = url.to_file_path().ok()?;
    let source = fs::read_to_string(&path)
        .map_err(
            |err| tracing::warn!(path = %path.display(), error = %err, "failed to read stylesheet"),
        )
        .ok()?;
    let uri = url.as_str().parse().ok()?;
    Some(parse_stylesheet(uri, source))
}

fn resolve_remote_stylesheet(cache: &StylesheetCache, url: &Url) -> Option<CachedStylesheet> {
    let key = url.as_str().to_owned();

    let cell = match cache.read() {
        Ok(guard) => guard.get(&key).cloned(),
        Err(err) => {
            tracing::error!(error = %err, "stylesheet cache lock poisoned");
            return None;
        }
    };

    let cell = if let Some(existing) = cell {
        existing
    } else {
        let mut guard = match cache.write() {
            Ok(guard) => guard,
            Err(err) => {
                tracing::error!(error = %err, "stylesheet cache lock poisoned");
                return None;
            }
        };
        guard
            .entry(key)
            .or_insert_with(|| Arc::new(OnceLock::new()))
            .clone()
    };

    cell.get_or_init(|| {
        let stylesheet_url = url.as_str();
        let agent = ureq::Agent::new_with_config(
            ureq::config::Config::builder()
                .timeout_global(Some(Duration::from_secs(10)))
                .build(),
        );
        let source = agent
            .get(stylesheet_url)
            .call()
            .map_err(|err| tracing::warn!(url = stylesheet_url, error = %err, "stylesheet fetch failed"))
            .ok()?
            .body_mut()
            .read_to_string()
            .map_err(|err| tracing::warn!(url = stylesheet_url, error = %err, "failed to read stylesheet body"))
            .ok()?;
        let uri = stylesheet_url.parse().ok()?;
        Some(parse_stylesheet(uri, source))
    })
    .clone()
}

pub fn resolve_external_stylesheet(
    cache: &StylesheetCache,
    base_uri: &Uri,
    href: &str,
) -> Option<(CachedStylesheet, bool)> {
    let url = resolve_stylesheet_url(base_uri, href)?;
    match url.scheme() {
        "file" => resolve_file_stylesheet(&url).map(|sheet| (sheet, false)),
        "http" | "https" => resolve_remote_stylesheet(cache, &url).map(|sheet| (sheet, true)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn resolve_stylesheet_url_handles_relative_file_href() -> TestResult {
        let base: Uri = "file:///tmp/example.svg".parse()?;
        let resolved = resolve_stylesheet_url(&base, "styles/site.css").ok_or("resolved")?;

        assert_eq!(resolved.as_str(), "file:///tmp/styles/site.css");
        Ok(())
    }

    #[test]
    fn resolve_file_stylesheet_collects_class_definitions() -> TestResult {
        let temp_dir = tempfile::tempdir()?;
        let css_path = temp_dir.path().join("style.css");
        fs::write(&css_path, ".uses-color { fill: red; }")?;

        let url = Url::from_file_path(&css_path).map_err(|()| "file url")?;
        let stylesheet = resolve_file_stylesheet(&url).ok_or("stylesheet")?;

        assert_eq!(
            stylesheet
                .class_definitions
                .iter()
                .map(|definition| definition.name.as_str())
                .collect::<Vec<_>>(),
            vec!["uses-color"]
        );

        Ok(())
    }
}
