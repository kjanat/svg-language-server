mod suppressions;

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
};

use suppressions::Suppressions;
use svg_data::{CompatVerdict, ProfileLookup, SpecLifecycle, VerdictReason};
use svg_tree::{is_attribute_name_kind, walk_tree};
use tree_sitter::{Node, Tree};

use crate::{
    namespaces::{self, NamespaceScope, SVG_NAMESPACE_URI, XLINK_NAMESPACE_URI},
    types::{CompatFlags, DiagnosticCode, LintOptions, LintOverrides, Severity, SvgDiagnostic},
};

/// Element-vs-attribute specific diagnostic codes for lifecycle-driven
/// rules. Shared advisory codes (partial/prefix/flag) are not in here
/// because they don't split by kind.
#[derive(Clone, Copy)]
struct LifecycleCodes {
    deprecated: DiagnosticCode,
    obsolete: DiagnosticCode,
    experimental: DiagnosticCode,
}

/// Bundled input for a single lifecycle-driven diagnostic emission.
///
/// Grouping these four fields keeps [`emit_lifecycle_diag_in_tag`]
/// under the arg-count budget and lets callers read as `emit(.., diag)`
/// rather than `emit(.., lifecycle, verdict, codes, subject)`.
#[derive(Clone, Copy)]
struct LifecycleDiagnostic<'a> {
    lifecycle: SpecLifecycle,
    /// Pre-computed verdict for richer messages. `None` when the catalog
    /// doesn't carry verdicts (e.g. in unit-test fixtures).
    verdict: Option<CompatVerdict>,
    codes: LifecycleCodes,
    subject: &'a str,
}

const ELEMENT_LIFECYCLE_CODES: LifecycleCodes = LifecycleCodes {
    deprecated: DiagnosticCode::DeprecatedElement,
    obsolete: DiagnosticCode::ObsoleteElement,
    experimental: DiagnosticCode::ExperimentalElement,
};

const ATTRIBUTE_LIFECYCLE_CODES: LifecycleCodes = LifecycleCodes {
    deprecated: DiagnosticCode::DeprecatedAttribute,
    obsolete: DiagnosticCode::ObsoleteAttribute,
    experimental: DiagnosticCode::ExperimentalAttribute,
};

struct LintContext<'a> {
    source: &'a [u8],
    diagnostics: Vec<SvgDiagnostic>,
    suppressions: Suppressions,
    defined_ids: HashSet<String>,
    seen_ids: HashMap<String, usize>,
    options: LintOptions,
    overrides: Option<&'a LintOverrides>,
}

/// Run all lint checks on a parsed SVG tree.
pub fn check_all(
    source: &[u8],
    tree: &Tree,
    options: LintOptions,
    overrides: Option<&LintOverrides>,
) -> Vec<SvgDiagnostic> {
    let mut ctx = LintContext {
        source,
        diagnostics: Vec::new(),
        suppressions: suppressions::collect_suppressions(source, tree),
        defined_ids: collect_defined_ids(source, tree),
        seen_ids: HashMap::new(),
        options,
        overrides,
    };
    walk_elements(&mut ctx, tree.root_node(), &NamespaceScope::default());
    ctx.diagnostics
        .extend(ctx.suppressions.unused_diagnostics());
    ctx.diagnostics
}

fn walk_elements<'a>(ctx: &mut LintContext<'a>, node: Node, parent_scope: &NamespaceScope<'a>) {
    let kind = node.kind();
    let child_scope = if kind == "element" || kind == "svg_root_element" {
        check_element(ctx, node, parent_scope)
    } else {
        parent_scope.clone()
    };
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_elements(ctx, child, &child_scope);
    }
}

fn check_element<'a>(
    ctx: &mut LintContext<'a>,
    node: Node,
    parent_scope: &NamespaceScope<'a>,
) -> NamespaceScope<'a> {
    // Find the opening tag node (start_tag or self_closing_tag)
    let Some(tag) = opening_tag(node) else {
        return parent_scope.clone();
    };

    // Extract element name from tag's `name` field
    let Some(name_node) = tag.child_by_field_name("name") else {
        return parent_scope.clone();
    };
    let name_str = std::str::from_utf8(&ctx.source[name_node.byte_range()]).unwrap_or("");
    let mut scope = namespaces::scope_for_tag(ctx.source, tag, parent_scope);
    let (prefix, local_name) = namespaces::split_qualified_name(name_str);
    if node.kind() == "svg_root_element"
        && prefix.is_none()
        && local_name == "svg"
        && scope.default_namespace().is_none()
        && !namespaces::declares_default_namespace(ctx.source, tag)
    {
        scope.set_default_namespace(Some(SVG_NAMESPACE_URI));
    }
    let expanded_name = namespaces::expand_element_name(name_str, &scope);

    if expanded_name.namespace_uri != Some(SVG_NAMESPACE_URI) {
        return scope;
    }

    let lookup = svg_data::element_for_profile(ctx.options.profile, expanded_name.local_name);
    let def = match lookup {
        ProfileLookup::Present { value, lifecycle } => {
            let lifecycle =
                element_diagnostic_lifecycle(ctx, expanded_name.local_name, value, lifecycle);
            emit_element_compat_diags(ctx, name_node, value, lifecycle);
            value
        }
        ProfileLookup::UnsupportedInProfile { .. } => {
            push_diag(
                &mut ctx.diagnostics,
                &mut ctx.suppressions,
                name_node,
                Severity::Error,
                DiagnosticCode::UnsupportedInProfile,
                format!(
                    "SVG element <{}> is not available in {}",
                    expanded_name.local_name,
                    ctx.options.profile.as_str()
                ),
            );
            return scope;
        }
        ProfileLookup::Unknown => {
            push_diag(
                &mut ctx.diagnostics,
                &mut ctx.suppressions,
                name_node,
                Severity::Error,
                DiagnosticCode::UnknownElement,
                unknown_element_message(expanded_name.local_name),
            );
            return scope;
        }
    };

    // Check: Unknown/deprecated/experimental attributes
    check_attributes(ctx, tag, &scope);

    // Check: Duplicate id
    check_duplicate_id(
        ctx.source,
        tag,
        &mut ctx.diagnostics,
        &mut ctx.suppressions,
        &mut ctx.seen_ids,
    );

    // Check: Missing local fragment reference definitions
    check_missing_reference_definitions(
        ctx.source,
        tag,
        &scope,
        &mut ctx.diagnostics,
        &mut ctx.suppressions,
        &ctx.defined_ids,
    );

    // Check: Invalid children
    check_children(
        ctx.source,
        node,
        def.name,
        &scope,
        ctx.options,
        &mut ctx.diagnostics,
        &mut ctx.suppressions,
    );

    scope
}

/// XML infrastructure prefixes — skip these in attribute checks.
fn is_xml_infrastructure(name: &str) -> bool {
    name == "xmlns" || name.starts_with("xmlns:") || name.starts_with("xml:")
}

fn check_attributes(ctx: &mut LintContext<'_>, tag: Node, scope: &NamespaceScope<'_>) {
    let tag_start = tag.start_position().row;
    let mut cursor = tag.walk();
    for attr_node in tag.children(&mut cursor) {
        if attr_node.kind() != "attribute" {
            continue;
        }
        // Find the attribute name node inside the (possibly typed) attribute
        let name_node = find_attr_name(attr_node);
        let Some(name_node) = name_node else {
            continue;
        };
        let attr_name = std::str::from_utf8(&ctx.source[name_node.byte_range()]).unwrap_or("");
        if attr_name.is_empty() || is_xml_infrastructure(attr_name) {
            continue;
        }
        let expanded_name = namespaces::expand_attribute_name(attr_name, scope);
        let lookup_name = canonical_svg_attribute_name(attr_name, expanded_name);
        let Some(lookup_name) = lookup_name else {
            continue;
        };

        // Generic attribute names are a mixed bucket of valid SVG attributes and truly
        // unknown ones. Without a complete checked-in attribute catalog, treating a catalog
        // miss as "unknown" makes diagnostics depend on build-time BCD fetch state.
        match svg_data::attribute_for_profile(ctx.options.profile, lookup_name.as_ref()) {
            ProfileLookup::Present { value, lifecycle } => {
                let lifecycle =
                    attribute_diagnostic_lifecycle(ctx, lookup_name.as_ref(), value, lifecycle);
                let verdict = svg_data::compat_verdict_for_attribute(value, ctx.options.profile);
                emit_lifecycle_diag_in_tag(
                    &mut ctx.diagnostics,
                    &mut ctx.suppressions,
                    name_node,
                    Some(tag_start),
                    LifecycleDiagnostic {
                        lifecycle,
                        verdict,
                        codes: ATTRIBUTE_LIFECYCLE_CODES,
                        subject: value.name,
                    },
                );
                emit_verdict_hints(
                    &mut ctx.diagnostics,
                    &mut ctx.suppressions,
                    name_node,
                    Some(tag_start),
                    verdict,
                    value.name,
                );
            }
            ProfileLookup::UnsupportedInProfile { .. } => {
                push_diag_in_tag(
                    &mut ctx.diagnostics,
                    &mut ctx.suppressions,
                    name_node,
                    Some(tag_start),
                    Severity::Error,
                    DiagnosticCode::UnsupportedInProfile,
                    format!(
                        "SVG attribute {} is not available in {}",
                        lookup_name,
                        ctx.options.profile.as_str()
                    ),
                );
            }
            ProfileLookup::Unknown => {}
        }
    }
}

/// Walk into an `attribute` node to find the name node.
fn find_attr_name(attr_node: Node) -> Option<Node> {
    let mut cursor = attr_node.walk();
    for child in attr_node.children(&mut cursor) {
        // Check if this child itself is a name node
        if is_attribute_name_kind(child.kind()) {
            return Some(child);
        }
        // Check the child's children (typed attributes nest name inside)
        let mut inner_cursor = child.walk();
        for grandchild in child.children(&mut inner_cursor) {
            if is_attribute_name_kind(grandchild.kind()) {
                return Some(grandchild);
            }
        }
    }
    None
}

fn check_duplicate_id(
    source: &[u8],
    tag: Node,
    diagnostics: &mut Vec<SvgDiagnostic>,
    suppressions: &mut Suppressions,
    seen_ids: &mut HashMap<String, usize>,
) {
    let tag_start = tag.start_position().row;
    let mut cursor = tag.walk();
    for attr_node in tag.children(&mut cursor) {
        if attr_node.kind() != "attribute" {
            continue;
        }
        let mut attr_cursor = attr_node.walk();
        for child in attr_node.children(&mut attr_cursor) {
            if child.kind() != "id_attribute" {
                continue;
            }
            let Some(value_node) = child.child_by_field_name("value") else {
                continue;
            };
            let mut vc = value_node.walk();
            for v in value_node.children(&mut vc) {
                if v.kind() != "id_token" {
                    continue;
                }
                let id_text = std::str::from_utf8(&source[v.byte_range()]).unwrap_or("");
                if let Some(&first_row) = seen_ids.get(id_text) {
                    push_diag_in_tag(
                        diagnostics,
                        suppressions,
                        v,
                        Some(tag_start),
                        Severity::Warning,
                        DiagnosticCode::DuplicateId,
                        format!(
                            "Duplicate id \"{id_text}\" (first on line {})",
                            first_row + 1
                        ),
                    );
                } else {
                    seen_ids.insert(id_text.to_string(), v.start_position().row);
                }
            }
        }
    }
}

fn check_children(
    source: &[u8],
    parent_node: Node,
    parent_name: &str,
    parent_scope: &NamespaceScope<'_>,
    options: LintOptions,
    diagnostics: &mut Vec<SvgDiagnostic>,
    suppressions: &mut Suppressions,
) {
    if svg_data::allows_foreign_children(parent_name) {
        return;
    }

    let allowed = svg_data::allowed_children_with_profile(options.profile, parent_name);

    let mut cursor = parent_node.walk();
    for child in parent_node.children(&mut cursor) {
        if child.kind() != "element" {
            continue;
        }
        let mut child_cursor = child.walk();
        let child_tag = child
            .children(&mut child_cursor)
            .find(|c| c.kind() == "start_tag" || c.kind() == "self_closing_tag");
        let Some(ct) = child_tag else { continue };
        let Some(cn) = ct.child_by_field_name("name") else {
            continue;
        };
        let child_name = std::str::from_utf8(&source[cn.byte_range()]).unwrap_or("");
        let child_scope = namespaces::scope_for_tag(source, ct, parent_scope);
        let expanded_name = namespaces::expand_element_name(child_name, &child_scope);
        if expanded_name.namespace_uri != Some(SVG_NAMESPACE_URI) {
            continue;
        }

        let ProfileLookup::Present {
            value: child_element,
            ..
        } = svg_data::element_for_profile(options.profile, expanded_name.local_name)
        else {
            continue;
        };

        if !allowed
            .iter()
            .any(|allowed_child| allowed_child.element.name == child_element.name)
        {
            push_diag(
                diagnostics,
                suppressions,
                cn,
                Severity::Error,
                DiagnosticCode::InvalidChild,
                format!(
                    "<{}> is not allowed as a child of <{parent_name}>",
                    child_element.name
                ),
            );
        }
    }
}

fn check_missing_reference_definitions(
    source: &[u8],
    tag: Node,
    scope: &NamespaceScope<'_>,
    diagnostics: &mut Vec<SvgDiagnostic>,
    suppressions: &mut Suppressions,
    defined_ids: &HashSet<String>,
) {
    let tag_start = tag.start_position().row;
    let mut cursor = tag.walk();
    for attr_node in tag.children(&mut cursor) {
        if attr_node.kind() != "attribute" {
            continue;
        }

        let Some(name_node) = find_attr_name(attr_node) else {
            continue;
        };
        let attr_name = std::str::from_utf8(&source[name_node.byte_range()]).unwrap_or("");
        if attr_name.is_empty() || is_xml_infrastructure(attr_name) {
            continue;
        }
        if canonical_svg_attribute_name(
            attr_name,
            namespaces::expand_attribute_name(attr_name, scope),
        )
        .is_none()
        {
            continue;
        }

        let mut attr_cursor = attr_node.walk();
        walk_tree(&mut attr_cursor, &mut |node| {
            if node.kind() != "iri_reference" {
                return;
            }

            let Ok(reference_text) = node.utf8_text(source) else {
                return;
            };
            let Some(id) = reference_text.strip_prefix('#') else {
                return;
            };
            if defined_ids.contains(id) {
                return;
            }

            push_diag_in_tag(
                diagnostics,
                suppressions,
                node,
                Some(tag_start),
                Severity::Warning,
                DiagnosticCode::MissingReferenceDefinition,
                format!(
                    "{attr_name} references #{id}, but no element with id=\"{id}\" exists in this SVG.\nDefine one or remove the reference."
                ),
            );
        });
    }
}

fn canonical_svg_attribute_name<'a>(
    raw_name: &'a str,
    expanded_name: namespaces::ExpandedName<'a>,
) -> Option<Cow<'a, str>> {
    let (prefix, _) = namespaces::split_qualified_name(raw_name);
    match (prefix, expanded_name.namespace_uri) {
        (None, None | Some(SVG_NAMESPACE_URI)) => Some(Cow::Borrowed(expanded_name.local_name)),
        (Some("xlink"), Some(XLINK_NAMESPACE_URI)) => {
            Some(Cow::Owned(format!("xlink:{}", expanded_name.local_name)))
        }
        _ => None,
    }
}

fn opening_tag(node: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|child| child.kind() == "start_tag" || child.kind() == "self_closing_tag")
}

fn emit_lifecycle_diag(
    diagnostics: &mut Vec<SvgDiagnostic>,
    suppressions: &mut Suppressions,
    node: Node,
    lifecycle_diag: LifecycleDiagnostic<'_>,
) {
    emit_lifecycle_diag_in_tag(diagnostics, suppressions, node, None, lifecycle_diag);
}

/// Emit lifecycle + advisory diagnostics for an element matched as
/// present in the selected profile. Extracted so [`check_element`]
/// stays under the function-length budget without losing the
/// end-to-end flow (lifecycle → verdict → hints) at a single site.
fn emit_element_compat_diags(
    ctx: &mut LintContext<'_>,
    name_node: Node<'_>,
    value: &'static svg_data::ElementDef,
    lifecycle: SpecLifecycle,
) {
    let verdict = svg_data::compat_verdict_for_element(value, ctx.options.profile);
    let subject = format!("<{}>", value.name);
    emit_lifecycle_diag(
        &mut ctx.diagnostics,
        &mut ctx.suppressions,
        name_node,
        LifecycleDiagnostic {
            lifecycle,
            verdict,
            codes: ELEMENT_LIFECYCLE_CODES,
            subject: &subject,
        },
    );
    emit_verdict_hints(
        &mut ctx.diagnostics,
        &mut ctx.suppressions,
        name_node,
        None,
        verdict,
        &subject,
    );
}

fn element_diagnostic_lifecycle(
    ctx: &LintContext<'_>,
    element_name: &str,
    value: &svg_data::ElementDef,
    lifecycle: SpecLifecycle,
) -> SpecLifecycle {
    diagnostic_lifecycle(
        lifecycle,
        effective_catalog_flags(ctx.options.profile, value.deprecated, value.experimental),
        ctx.overrides
            .and_then(|overrides| overrides.elements.get(element_name)),
    )
}

fn attribute_diagnostic_lifecycle(
    ctx: &LintContext<'_>,
    attribute_name: &str,
    value: &svg_data::AttributeDef,
    lifecycle: SpecLifecycle,
) -> SpecLifecycle {
    diagnostic_lifecycle(
        lifecycle,
        effective_catalog_flags(ctx.options.profile, value.deprecated, value.experimental),
        ctx.overrides
            .and_then(|overrides| overrides.attributes.get(attribute_name)),
    )
}

/// The catalog's baked `deprecated` / `experimental` flags come from BCD,
/// which encodes latest-era advice — "don't use this in new web work".
/// When the caller selected a non-latest profile they are deliberately
/// targeting an older spec where those flags don't apply (the canonical
/// example: `xlink:href` was the standard linking attribute in SVG 1.1;
/// BCD's deprecation reflects its SVG 2 removal). Zero the flags in that
/// case so diagnostic promotion only fires under the latest profile.
///
/// Runtime overrides are applied downstream and are unaffected — they're
/// intentional user-set signals, not catalog-baked ones.
fn effective_catalog_flags(
    profile: svg_data::SpecSnapshotId,
    deprecated: bool,
    experimental: bool,
) -> CompatFlags {
    if profile == svg_data::SpecSnapshotId::LATEST {
        CompatFlags {
            deprecated,
            experimental,
        }
    } else {
        CompatFlags {
            deprecated: false,
            experimental: false,
        }
    }
}

fn unknown_element_message(name: &str) -> String {
    let mut msg = format!("Unknown SVG element: <{name}>");
    if let Some(suggestion) = closest_name(name, svg_data::elements().iter().map(|e| e.name)) {
        use std::fmt::Write;
        let _ = write!(msg, "\nDid you mean <{suggestion}>?");
    }
    msg
}

fn emit_lifecycle_diag_in_tag(
    diagnostics: &mut Vec<SvgDiagnostic>,
    suppressions: &mut Suppressions,
    node: Node,
    tag_start: Option<usize>,
    lifecycle_diag: LifecycleDiagnostic<'_>,
) {
    let LifecycleDiagnostic {
        lifecycle,
        verdict,
        codes,
        subject,
    } = lifecycle_diag;
    match lifecycle {
        SpecLifecycle::Deprecated => push_diag_in_tag(
            diagnostics,
            suppressions,
            node,
            tag_start,
            Severity::Warning,
            codes.deprecated,
            format_deprecated_message(verdict, subject),
        ),
        SpecLifecycle::Obsolete => push_diag_in_tag(
            diagnostics,
            suppressions,
            node,
            tag_start,
            Severity::Warning,
            codes.obsolete,
            format_obsolete_message(verdict, subject),
        ),
        SpecLifecycle::Experimental => push_diag_in_tag(
            diagnostics,
            suppressions,
            node,
            tag_start,
            Severity::Information,
            codes.experimental,
            format!("{subject} is experimental"),
        ),
        SpecLifecycle::Stable => {}
    }
}

/// Compose a deprecated-rule message that matches the hover `Status:`
/// line. When the verdict names `BcdDeprecated`, we say so explicitly;
/// otherwise we fall back to the terse legacy message. Both surfaces
/// read from the same verdict, so they cannot drift.
fn format_deprecated_message(verdict: Option<CompatVerdict>, subject: &str) -> String {
    if let Some(v) = verdict
        && v.reasons
            .iter()
            .any(|r| matches!(r, VerdictReason::BcdDeprecated))
    {
        return format!("{subject} is deprecated (BCD-deprecated)");
    }
    format!("{subject} is deprecated")
}

/// Compose an obsolete-rule message. This branch only fires when the
/// feature is still in the selected profile's membership but its union
/// lifecycle is marked `Obsolete` — otherwise the walk would have
/// taken the `UnsupportedInProfile` path and never reached here.
///
/// `VerdictReason::ProfileObsolete` can never coexist with this branch
/// (it's emitted only for membership-absent features), so we reinforce
/// with `BcdDeprecated` when present to mirror the hover Status line.
fn format_obsolete_message(verdict: Option<CompatVerdict>, subject: &str) -> String {
    let also_bcd = verdict.is_some_and(|v| {
        v.reasons
            .iter()
            .any(|r| matches!(r, VerdictReason::BcdDeprecated))
    });
    if also_bcd {
        format!("{subject} is obsolete in the current SVG profile (also BCD-deprecated)")
    } else {
        format!("{subject} is obsolete in the current SVG profile")
    }
}

/// Emit info-severity advisory diagnostics for compat caveats that
/// aren't deprecation (partial implementation, vendor prefix, runtime
/// flag). These read directly from [`CompatVerdict::reasons`] so they
/// stay in lockstep with the hover status line.
///
/// Duplicate reasons (e.g. `PartialImplementationIn` for both Chrome
/// and Edge on `color-interpolation`) collapse into a single bullet-
/// separated diagnostic per rule code — the LSP's "problems" panel is
/// noisy enough without four near-identical entries per attribute.
fn emit_verdict_hints(
    diagnostics: &mut Vec<SvgDiagnostic>,
    suppressions: &mut Suppressions,
    node: Node,
    tag_start: Option<usize>,
    verdict: Option<CompatVerdict>,
    subject: &str,
) {
    let Some(verdict) = verdict else { return };

    let mut partial = Vec::new();
    let mut prefix = Vec::new();
    let mut flagged = Vec::new();

    for reason in verdict.reasons {
        match reason {
            VerdictReason::PartialImplementationIn(browser) => partial.push(*browser),
            VerdictReason::PrefixRequiredIn { browser, prefix: p } => {
                prefix.push(format!("{browser} (`{p}`)"));
            }
            VerdictReason::BehindFlagIn(browser) => flagged.push(*browser),
            _ => {}
        }
    }

    if !partial.is_empty() {
        push_diag_in_tag(
            diagnostics,
            suppressions,
            node,
            tag_start,
            Severity::Information,
            DiagnosticCode::PartialImplementation,
            format!(
                "{subject} has a partial implementation in {}",
                partial.join(", ")
            ),
        );
    }

    if !prefix.is_empty() {
        push_diag_in_tag(
            diagnostics,
            suppressions,
            node,
            tag_start,
            Severity::Information,
            DiagnosticCode::PrefixRequired,
            format!(
                "{subject} requires a vendor prefix in {}",
                prefix.join(", ")
            ),
        );
    }

    if !flagged.is_empty() {
        push_diag_in_tag(
            diagnostics,
            suppressions,
            node,
            tag_start,
            Severity::Information,
            DiagnosticCode::BehindFlag,
            format!(
                "{subject} is only available behind a flag in {}",
                flagged.join(", ")
            ),
        );
    }
}

fn diagnostic_lifecycle(
    spec_lifecycle: SpecLifecycle,
    compat_flags: CompatFlags,
    override_flags: Option<&CompatFlags>,
) -> SpecLifecycle {
    if matches!(
        spec_lifecycle,
        SpecLifecycle::Deprecated | SpecLifecycle::Obsolete
    ) {
        return spec_lifecycle;
    }

    let deprecated = override_flags.map_or(compat_flags.deprecated, |flags| flags.deprecated);
    if deprecated {
        return SpecLifecycle::Deprecated;
    }

    if spec_lifecycle == SpecLifecycle::Experimental {
        return spec_lifecycle;
    }

    let experimental = override_flags.map_or(compat_flags.experimental, |flags| flags.experimental);
    if experimental {
        SpecLifecycle::Experimental
    } else {
        SpecLifecycle::Stable
    }
}

fn collect_defined_ids(source: &[u8], tree: &Tree) -> HashSet<String> {
    let mut ids = HashSet::new();
    let mut cursor = tree.root_node().walk();
    walk_tree(&mut cursor, &mut |node| {
        if node.kind() != "id_token" {
            return;
        }
        if let Ok(id) = node.utf8_text(source) {
            ids.insert(id.to_owned());
        }
    });
    ids
}

fn push_diag(
    diagnostics: &mut Vec<SvgDiagnostic>,
    suppressions: &mut Suppressions,
    node: Node,
    severity: Severity,
    code: DiagnosticCode,
    message: String,
) {
    push_diag_in_tag(
        diagnostics,
        suppressions,
        node,
        None,
        severity,
        code,
        message,
    );
}

/// Like `push_diag` but accepts the enclosing tag's start row so that a
/// `disable-next-line` before a multiline opening tag covers all attributes.
fn push_diag_in_tag(
    diagnostics: &mut Vec<SvgDiagnostic>,
    suppressions: &mut Suppressions,
    node: Node,
    tag_start_row: Option<usize>,
    severity: Severity,
    code: DiagnosticCode,
    message: String,
) {
    if suppressions.suppresses(node.start_position().row, code, tag_start_row) {
        return;
    }
    diagnostics.push(make_diag(node, severity, code, message));
}

fn make_diag(
    node: Node,
    severity: Severity,
    code: DiagnosticCode,
    message: String,
) -> SvgDiagnostic {
    SvgDiagnostic {
        byte_range: node.byte_range(),
        start_row: node.start_position().row,
        start_col: node.start_position().column,
        end_row: node.end_position().row,
        end_col: node.end_position().column,
        severity,
        code,
        message,
    }
}

/// Levenshtein edit distance between two strings.
fn edit_distance(a: &str, b: &str) -> usize {
    let b_len = b.len();
    let mut prev: Vec<usize> = (0..=b_len).collect();
    let mut curr = vec![0; b_len + 1];
    for (i, ca) in a.chars().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.chars().enumerate() {
            let cost = usize::from(ca != cb);
            curr[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(curr[j] + 1);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b_len]
}

/// Find the closest match from `candidates` for `name`, returning it only if
/// the edit distance is at most `max(2, name.len() / 3)`.
fn closest_name<'a>(name: &str, candidates: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let threshold = (name.len() / 3).max(2);
    candidates
        .map(|c| (c, edit_distance(name, c)))
        .filter(|(_, d)| *d > 0 && *d <= threshold)
        .min_by_key(|(_, d)| *d)
        .map(|(c, _)| c)
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use suppressions::Suppressions;
    use tree_sitter::{Parser, Tree};

    use super::*;

    #[test]
    fn edit_distance_basic() {
        assert_eq!(edit_distance("rect", "rect"), 0);
        assert_eq!(edit_distance("rectt", "rect"), 1);
        assert_eq!(edit_distance("rct", "rect"), 1);
        assert_eq!(edit_distance("banana", "rect"), 6);
    }

    #[test]
    fn closest_name_suggests_near_match() {
        let candidates = ["rect", "circle", "line", "path", "text"];
        assert_eq!(
            closest_name("rectt", candidates.iter().copied()),
            Some("rect")
        );
        assert_eq!(
            closest_name("cirle", candidates.iter().copied()),
            Some("circle")
        );
        assert_eq!(closest_name("banana", candidates.iter().copied()), None);
    }

    #[test]
    fn unknown_element_suggests_correction() {
        let src = br"<svg><rectt/></svg>";
        let diags = crate::lint(src);
        let unknown = diags
            .iter()
            .find(|d| d.code == DiagnosticCode::UnknownElement);
        assert!(unknown.is_some(), "should flag unknown element: {diags:?}");
        assert!(
            unknown.is_some_and(|d| d.message.contains("Did you mean")),
            "should suggest correction: {:?}",
            unknown.map(|d| &d.message)
        );
    }

    fn parse_svg(source: &str) -> Result<Tree, Box<dyn Error>> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_svg::LANGUAGE.into())
            .map_err(|e| format!("SVG grammar: {e}"))?;
        parser
            .parse(source, None)
            .ok_or_else(|| "parse returned None".into())
    }

    fn first_attribute_node(tree: &Tree) -> Result<Node<'_>, Box<dyn Error>> {
        fn visit(node: Node<'_>) -> Option<Node<'_>> {
            if node.kind() == "attribute" {
                return Some(node);
            }

            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if let Some(found) = visit(child) {
                    return Some(found);
                }
            }
            None
        }

        visit(tree.root_node()).ok_or_else(|| "expected an attribute node".into())
    }

    #[test]
    fn find_attr_name_matches_new_duration_attribute_kind() -> Result<(), Box<dyn Error>> {
        let tree = parse_svg(r#"<svg><animate dur="2s" /></svg>"#)?;
        let attr = first_attribute_node(&tree)?;
        let name = find_attr_name(attr).ok_or("duration attribute name")?;
        assert_eq!(name.kind(), "duration_attribute_name");
        Ok(())
    }

    #[test]
    fn find_attr_name_matches_new_stroke_dasharray_attribute_kind() -> Result<(), Box<dyn Error>> {
        let tree = parse_svg(r#"<svg><line stroke-dasharray="10 5" /></svg>"#)?;
        let attr = first_attribute_node(&tree)?;
        let name = find_attr_name(attr).ok_or("stroke-dasharray attribute name")?;
        assert_eq!(name.kind(), "stroke_dasharray_attribute_name");
        Ok(())
    }

    #[test]
    fn experimental_lifecycle_uses_information_severity() -> Result<(), Box<dyn Error>> {
        let tree = parse_svg(r"<svg><rect/></svg>")?;
        let root = tree.root_node();
        let mut cursor = root.walk();
        let svg = root
            .children(&mut cursor)
            .find(|child| child.kind() == "svg_root_element")
            .ok_or("svg root")?;
        let mut tag_cursor = svg.walk();
        let tag = svg
            .children(&mut tag_cursor)
            .find(|child| child.kind() == "start_tag")
            .ok_or("start tag")?;
        let name = tag.child_by_field_name("name").ok_or("tag name")?;
        let mut diagnostics = Vec::new();
        let mut suppressions = Suppressions::default();

        emit_lifecycle_diag(
            &mut diagnostics,
            &mut suppressions,
            name,
            LifecycleDiagnostic {
                lifecycle: SpecLifecycle::Experimental,
                verdict: None,
                codes: ELEMENT_LIFECYCLE_CODES,
                subject: "<svg>",
            },
        );

        assert_eq!(diagnostics.len(), 1, "expected one lifecycle diagnostic");
        assert_eq!(diagnostics[0].severity, Severity::Information);
        assert_eq!(diagnostics[0].code, DiagnosticCode::ExperimentalElement);
        Ok(())
    }
}
