use std::borrow::Cow;

pub fn canonical_svg_attribute_name(name: &str) -> Cow<'_, str> {
    match name {
        "xlink_actuate" => Cow::Borrowed("xlink:actuate"),
        "xlink_arcrole" => Cow::Borrowed("xlink:arcrole"),
        "xlink_href" => Cow::Borrowed("xlink:href"),
        "xlink_role" => Cow::Borrowed("xlink:role"),
        "xlink_show" => Cow::Borrowed("xlink:show"),
        "xlink_title" => Cow::Borrowed("xlink:title"),
        "xlink_type" => Cow::Borrowed("xlink:type"),
        _ => Cow::Borrowed(name),
    }
}
