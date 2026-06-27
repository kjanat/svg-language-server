//! The SVG Native profile and its constraints.

/// What kind of spec entity a constraint targets.
///
/// # Examples
///
/// ```rust
/// let kind = svg_data::profile::ConstraintKind::Element;
/// assert!(matches!(kind, svg_data::profile::ConstraintKind::Element));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstraintKind {
    /// An element.
    Element,
    /// An attribute.
    Attribute,
    /// A property.
    Property,
}

/// The scope an SVG Native conditional-support constraint applies across.
///
/// # Examples
///
/// ```rust
/// let scope = svg_data::profile::ConstraintScope::Elements { names: &["svg"] };
/// let svg_data::profile::ConstraintScope::Elements { names } = scope;
/// assert_eq!(names, &["svg"]);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstraintScope {
    /// The attribute/property is supported only on the listed bearer elements.
    Elements {
        /// Allowlisted bearer element names.
        names: &'static [&'static str],
    },
}

/// The SVG Native profile: the restricted subset of SVG that the SVG Native
/// rendering profile supports.
///
/// # Examples
///
/// ```rust
/// let native = svg_data::profile::svg_native();
/// assert!(native.unsupported_elements.contains(&"clipPath"));
/// ```
#[derive(Debug, PartialEq, Eq)]
pub struct SvgNative {
    /// Element names not supported by the profile.
    pub unsupported_elements: &'static [&'static str],
    /// Attribute names not supported by the profile.
    pub unsupported_attributes: &'static [&'static str],
    /// Property names not supported by the profile.
    pub unsupported_properties: &'static [&'static str],
}

impl SvgNative {
    /// Whether `name` (of the given kind) is unsupported by the profile.
    ///
    /// # Examples
    ///
    /// ```rust
    /// let native = svg_data::profile::svg_native();
    /// assert!(native.is_unsupported(svg_data::profile::ConstraintKind::Element, "clipPath"));
    /// ```
    #[must_use]
    pub fn is_unsupported(&self, kind: ConstraintKind, name: &str) -> bool {
        let set = match kind {
            ConstraintKind::Element => self.unsupported_elements,
            ConstraintKind::Attribute => self.unsupported_attributes,
            ConstraintKind::Property => self.unsupported_properties,
        };
        set.contains(&name)
    }

    /// The scope `name` (of `kind`) is conditionally restricted to, when SVG
    /// Native supports it only on a subset of bearers.
    ///
    /// # Examples
    ///
    /// ```rust
    /// let native = svg_data::profile::svg_native();
    /// let scope = native.supported_only(svg_data::profile::ConstraintKind::Attribute, "href");
    /// assert!(scope.is_none());
    /// ```
    #[must_use]
    pub const fn supported_only(
        &self,
        kind: ConstraintKind,
        name: &str,
    ) -> Option<ConstraintScope> {
        let _ = (kind, name);
        // Populated by the extraction pipeline; `None` until it lands.
        None
    }
}

/// The SVG Native profile data (extracted from the SVG Native spec).
///
/// # Examples
///
/// ```rust
/// let native = svg_data::profile::svg_native();
/// assert!(native.is_unsupported(svg_data::profile::ConstraintKind::Element, "clipPath"));
/// ```
#[must_use]
pub fn svg_native() -> &'static SvgNative {
    // Populated by the extraction pipeline; empty constraints until it lands.
    static SVG_NATIVE: SvgNative = SvgNative {
        unsupported_elements: &["clipPath"],
        unsupported_attributes: &["clip-path"],
        unsupported_properties: &[],
    };
    &SVG_NATIVE
}
