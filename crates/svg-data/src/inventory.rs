//! Per-edition element/attribute inventories and edition identifiers.

use std::borrow::Cow;

use crate::{SpecSnapshotId, edition::Series};

/// The publication date of an edition.
///
/// # Examples
///
/// ```rust
/// let date = svg_data::inventory::EditionDate::EditorsDraft;
/// assert!(matches!(date, svg_data::inventory::EditionDate::EditorsDraft));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EditionDate {
    /// A dated `/TR/` edition (`YYYY-MM-DD`).
    Dated {
        /// ISO date string (borrowed for baked editions, owned when parsed at
        /// runtime from an LSP config).
        date: Cow<'static, str>,
    },
    /// The rolling editor's draft (no dated URL).
    EditorsDraft,
}

/// A stable identifier for a specification edition.
///
/// # Examples
///
/// ```rust
/// let id = svg_data::inventory::EditionId::editors_draft(svg_data::edition::Series::Svg2);
/// assert_eq!(id.series, svg_data::edition::Series::Svg2);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EditionId {
    /// The series this edition belongs to.
    pub series: Series,
    /// The edition's publication date (or editor's draft).
    pub date: EditionDate,
}

impl EditionId {
    /// Construct a dated edition id.
    ///
    /// # Examples
    ///
    /// ```rust
    /// let id = svg_data::inventory::EditionId::dated(svg_data::edition::Series::Svg2, "2018-10-04");
    /// assert_eq!(id.series, svg_data::edition::Series::Svg2);
    /// ```
    #[must_use]
    pub const fn dated(series: Series, date: &'static str) -> Self {
        Self {
            series,
            date: EditionDate::Dated {
                date: Cow::Borrowed(date),
            },
        }
    }

    /// Construct the rolling editor's-draft edition id for a series.
    ///
    /// # Examples
    ///
    /// ```rust
    /// let id = svg_data::inventory::EditionId::editors_draft(svg_data::edition::Series::Svg2);
    /// assert_eq!(id.series, svg_data::edition::Series::Svg2);
    /// ```
    #[must_use]
    pub const fn editors_draft(series: Series) -> Self {
        Self {
            series,
            date: EditionDate::EditorsDraft,
        }
    }

    /// The edition id corresponding to a snapshot.
    ///
    /// # Examples
    ///
    /// ```rust
    /// let id = svg_data::inventory::EditionId::for_snapshot(svg_data::SpecSnapshotId::Svg2EditorsDraft);
    /// assert_eq!(id.series, svg_data::edition::Series::Svg2);
    /// ```
    #[must_use]
    pub const fn for_snapshot(snapshot: SpecSnapshotId) -> Self {
        match snapshot {
            SpecSnapshotId::Svg11Rec20030114 => Self::dated(Series::Svg11, "2003-01-14"),
            SpecSnapshotId::Svg11Rec20110816 => Self::dated(Series::Svg11, "2011-08-16"),
            SpecSnapshotId::Svg2Cr20181004 => Self::dated(Series::Svg2, "2018-10-04"),
            SpecSnapshotId::Svg2EditorsDraft => Self {
                series: Series::Svg2,
                date: EditionDate::EditorsDraft,
            },
        }
    }
}

/// An attribute an edition declares for an element.
///
/// # Examples
///
/// ```rust
/// let attr = svg_data::inventory::Attribute { name: "version" };
/// assert_eq!(attr.name, "version");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attribute {
    /// Attribute name.
    pub name: &'static str,
}

/// An element an edition declares, with the attributes it lists for it.
///
/// # Examples
///
/// ```rust
/// let element = svg_data::inventory::Element { name: "svg", attributes: &[] };
/// assert_eq!(element.name, "svg");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Element {
    /// Element name.
    pub name: &'static str,
    /// Attributes the edition lists for this element.
    pub attributes: &'static [Attribute],
}

/// The element/attribute inventory present in one edition.
///
/// # Examples
///
/// ```rust
/// let inventory = svg_data::inventory::for_edition(&svg_data::inventory::EditionId::dated(
///     svg_data::edition::Series::Svg10,
///     "2001-09-04",
/// ))
/// .expect("baked SVG 1.0 inventory");
/// assert!(inventory.elements.iter().any(|element| element.name == "svg"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inventory {
    /// The edition this inventory describes.
    pub edition: EditionId,
    /// Elements present in the edition.
    pub elements: &'static [Element],
}

impl Inventory {
    /// The attributes this edition lists for `elem_name` (empty if the element
    /// is absent or carries no listed attributes).
    ///
    /// # Examples
    ///
    /// ```rust
    /// let id = svg_data::inventory::EditionId::dated(svg_data::edition::Series::Svg10, "2001-09-04");
    /// let inventory = svg_data::inventory::for_edition(&id).expect("baked SVG 1.0 inventory");
    /// let names: Vec<_> = inventory.attributes_for_element("svg").map(|attr| attr.name).collect();
    /// assert!(names.contains(&"version"));
    /// ```
    pub fn attributes_for_element(
        &self,
        elem_name: &str,
    ) -> impl Iterator<Item = &'static Attribute> {
        self.elements
            .iter()
            .find(move |element| element.name == elem_name)
            .into_iter()
            .flat_map(|element| element.attributes.iter())
    }
}

static SVG10_20010904_SVG_ATTRIBUTES: &[Attribute] = &[Attribute { name: "version" }];

static SVG10_20010904_ELEMENTS: &[Element] = &[
    Element {
        name: "svg",
        attributes: SVG10_20010904_SVG_ATTRIBUTES,
    },
    Element {
        name: "definition-src",
        attributes: &[],
    },
];

static SVG10_20010904: Inventory = Inventory {
    edition: EditionId::dated(Series::Svg10, "2001-09-04"),
    elements: SVG10_20010904_ELEMENTS,
};

static SVG2_20160915: Inventory = Inventory {
    edition: EditionId::dated(Series::Svg2, "2016-09-15"),
    elements: &[],
};

/// The inventory for an edition, when one has been extracted.
///
/// # Examples
///
/// ```rust
/// let id = svg_data::inventory::EditionId::dated(svg_data::edition::Series::Svg10, "2001-09-04");
/// assert!(svg_data::inventory::for_edition(&id).is_some());
/// ```
#[must_use]
pub fn for_edition(edition: &EditionId) -> Option<&'static Inventory> {
    crate::catalog::INVENTORIES
        .iter()
        .find(|inventory| &inventory.edition == edition)
        .or_else(|| (edition == &SVG10_20010904.edition).then_some(&SVG10_20010904))
        .or_else(|| (edition == &SVG2_20160915.edition).then_some(&SVG2_20160915))
}

/// Generated inventories for curated snapshots.
///
/// # Examples
///
/// ```rust
/// let _generated = svg_data::inventory::generated();
/// ```
#[must_use]
pub const fn generated() -> &'static [Inventory] {
    crate::catalog::INVENTORIES
}
