use super::*;

#[derive(Clone)]
pub(crate) struct CachedStylesheet {
    pub(crate) uri: Uri,
    pub(crate) source: String,
    pub(crate) class_definitions: Vec<svg_references::NamedSpan>,
    pub(crate) custom_property_definitions: Vec<svg_references::NamedSpan>,
}

#[derive(Clone)]
pub(crate) struct ClassDefinitionHover {
    pub(crate) uri: Uri,
    pub(crate) source: String,
    pub(crate) definition: svg_references::NamedSpan,
}

impl ClassDefinitionHover {
    pub(crate) fn new(uri: Uri, source: String, definition: svg_references::NamedSpan) -> Self {
        Self {
            uri,
            source,
            definition,
        }
    }
}

#[derive(Clone)]
pub(crate) struct CustomPropertyDefinitionHover {
    pub(crate) uri: Uri,
    pub(crate) source: String,
    pub(crate) definition: svg_references::NamedSpan,
}

impl CustomPropertyDefinitionHover {
    pub(crate) fn new(uri: Uri, source: String, definition: svg_references::NamedSpan) -> Self {
        Self {
            uri,
            source,
            definition,
        }
    }
}

pub(crate) fn class_definition_hovers_from_stylesheet(
    uri: Uri,
    source: &str,
    target_class: &str,
) -> Vec<ClassDefinitionHover> {
    svg_references::collect_class_definitions_from_stylesheet(source, 0, 0)
        .into_iter()
        .filter(|definition| definition.name == target_class)
        .map(|definition| ClassDefinitionHover::new(uri.clone(), source.to_owned(), definition))
        .collect()
}

pub(crate) fn custom_property_definition_hovers_from_stylesheet(
    uri: Uri,
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

pub(crate) fn definition_response_from_locations(
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

pub(crate) fn resolve_stylesheet_url(base_uri: &Uri, href: &str) -> Option<Url> {
    if let Ok(url) = Url::parse(href) {
        return Some(url);
    }

    let base = Url::parse(base_uri.as_str()).ok()?;
    base.join(href).ok()
}

pub(crate) fn resolve_file_stylesheet(url: &Url) -> Option<CachedStylesheet> {
    let path = url.to_file_path().ok()?;
    let source = fs::read_to_string(path).ok()?;
    let uri = url.as_str().parse().ok()?;
    Some(parse_stylesheet(uri, source))
}

fn resolve_remote_stylesheet(cache: &StylesheetCache, url: &Url) -> Option<CachedStylesheet> {
    let key = url.as_str().to_owned();
    let cell = if let Ok(guard) = cache.read() {
        guard.get(&key).cloned()
    } else {
        None
    }
    .or_else(|| {
        let mut guard = cache.write().ok()?;
        Some(
            guard
                .entry(key)
                .or_insert_with(|| Arc::new(OnceLock::new()))
                .clone(),
        )
    })?;

    cell.get_or_init(|| {
        let source = ureq::get(url.as_str())
            .call()
            .ok()?
            .body_mut()
            .read_to_string()
            .ok()?;
        let uri = url.as_str().parse().ok()?;
        Some(parse_stylesheet(uri, source))
    })
    .clone()
}

pub(crate) fn resolve_external_stylesheet(
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
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn resolve_stylesheet_url_handles_relative_file_href() {
        let base: Uri = "file:///tmp/example.svg".parse().expect("uri");
        let resolved = resolve_stylesheet_url(&base, "styles/site.css").expect("resolved");

        assert_eq!(resolved.as_str(), "file:///tmp/styles/site.css");
    }

    #[test]
    fn resolve_file_stylesheet_collects_class_definitions() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("duration")
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("svg-ls-style-{unique}"));
        fs::create_dir_all(&temp_dir).expect("temp dir");
        let css_path = temp_dir.join("style.css");
        fs::write(&css_path, ".uses-color { fill: red; }").expect("css written");

        let url = Url::from_file_path(&css_path).expect("file url");
        let stylesheet = resolve_file_stylesheet(&url).expect("stylesheet");

        assert_eq!(
            stylesheet
                .class_definitions
                .iter()
                .map(|definition| definition.name.as_str())
                .collect::<Vec<_>>(),
            vec!["uses-color"]
        );

        fs::remove_file(&css_path).expect("cleanup css");
        fs::remove_dir(&temp_dir).expect("cleanup dir");
    }
}
