use tower_lsp_server::ls_types::{Location, Position, Uri};

use crate::{
    DocumentState,
    positions::{byte_offset_for_position, named_span_location},
    stylesheets::CachedStylesheet,
};

pub struct DefinitionContext {
    pub target: svg_references::DefinitionTarget,
    pub inline_locations: Vec<Location>,
    pub stylesheet_hrefs: Vec<String>,
}

pub fn build_definition_context(
    uri: &Uri,
    pos: Position,
    doc: &DocumentState,
) -> Option<DefinitionContext> {
    let source = doc.source.as_bytes();
    let byte_offset = byte_offset_for_position(source, pos);
    let target = svg_references::definition_target_at(source, &doc.tree, byte_offset)?;
    let (inline_locations, stylesheet_hrefs) =
        inline_definition_locations(uri, source, &doc.tree, &target);

    Some(DefinitionContext {
        target,
        inline_locations,
        stylesheet_hrefs,
    })
}

fn inline_definition_locations(
    uri: &Uri,
    source: &[u8],
    tree: &tree_sitter::Tree,
    target: &svg_references::DefinitionTarget,
) -> (Vec<Location>, Vec<String>) {
    match target {
        svg_references::DefinitionTarget::Id(target_id) => (
            svg_references::collect_id_definitions(source, tree)
                .into_iter()
                .filter(|definition| definition.name == *target_id)
                .map(|definition| named_span_location(uri.clone(), source, &definition))
                .collect(),
            Vec::new(),
        ),
        svg_references::DefinitionTarget::Class(target_class) => (
            svg_references::collect_inline_stylesheets(source, tree)
                .into_iter()
                .flat_map(|stylesheet| {
                    svg_references::collect_class_definitions_from_stylesheet(
                        &stylesheet.css,
                        stylesheet.start_row,
                        stylesheet.start_col,
                    )
                })
                .filter(|definition| definition.name == *target_class)
                .map(|definition| named_span_location(uri.clone(), source, &definition))
                .collect(),
            svg_references::extract_xml_stylesheet_hrefs(source),
        ),
        svg_references::DefinitionTarget::CustomProperty(target_property) => (
            svg_references::collect_inline_stylesheets(source, tree)
                .into_iter()
                .flat_map(|stylesheet| {
                    svg_references::collect_custom_property_definitions_from_stylesheet(
                        &stylesheet.css,
                        stylesheet.start_row,
                        stylesheet.start_col,
                    )
                })
                .filter(|definition| definition.name == *target_property)
                .map(|definition| named_span_location(uri.clone(), source, &definition))
                .collect(),
            svg_references::extract_xml_stylesheet_hrefs(source),
        ),
    }
}

pub fn stylesheet_definition_locations(
    stylesheet: &CachedStylesheet,
    target: &svg_references::DefinitionTarget,
) -> Vec<Location> {
    match target {
        svg_references::DefinitionTarget::Class(target_class) => stylesheet
            .class_definitions
            .iter()
            .filter(|definition| definition.name == *target_class)
            .map(|definition| {
                named_span_location(
                    stylesheet.uri.clone(),
                    stylesheet.source.as_bytes(),
                    definition,
                )
            })
            .collect(),
        svg_references::DefinitionTarget::CustomProperty(target_property) => stylesheet
            .custom_property_definitions
            .iter()
            .filter(|definition| definition.name == *target_property)
            .map(|definition| {
                named_span_location(
                    stylesheet.uri.clone(),
                    stylesheet.source.as_bytes(),
                    definition,
                )
            })
            .collect(),
        svg_references::DefinitionTarget::Id(_) => Vec::new(),
    }
}
