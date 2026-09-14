use crate::canvas::scene::{CanvasDocument, DEFAULT_STROKE_COLOR, ElementKind, StrokeStyle};

pub fn export_to_svg(doc: &CanvasDocument) -> String {
    let padding = 16.0;
    let (min_x, min_y, max_x, max_y) = doc.bounding_box().unwrap_or((0.0, 0.0, 400.0, 300.0));

    let vb_x = min_x - padding;
    let vb_y = min_y - padding;
    let vb_width = (max_x - min_x) + padding * 2.0;
    let vb_height = (max_y - min_y) + padding * 2.0;

    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{vb_x:.1} {vb_y:.1} {vb_width:.1} {vb_height:.1}\" width=\"{vb_width:.1}\" height=\"{vb_height:.1}\">\n"
    ));

    let mut arrow_colors: Vec<String> = doc
        .elements
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::Arrow { .. }))
        .map(|e| e.style.stroke_color.clone())
        .collect();
    arrow_colors.sort();
    arrow_colors.dedup();
    if arrow_colors.is_empty() {
        arrow_colors.push(DEFAULT_STROKE_COLOR.to_string());
    }

    svg.push_str("<defs>\n");
    for color in &arrow_colors {
        let clean_id = format!("arrowhead_{}", color.trim_start_matches('#'));
        svg.push_str(&format!(
            "  <marker id=\"{clean_id}\" viewBox=\"0 0 10 10\" refX=\"8\" refY=\"5\" markerWidth=\"6\" markerHeight=\"6\" orient=\"auto-start-reverse\">\n    <path d=\"M 0 0 L 10 5 L 0 10 z\" fill=\"{color}\" />\n  </marker>\n"
        ));
    }
    svg.push_str("</defs>\n");

    if let Some(bg) = &doc.background_color {
        svg.push_str(&format!(
            r#"<rect x="{vb_x:.1}" y="{vb_y:.1}" width="{vb_width:.1}" height="{vb_height:.1}" fill="{bg}" />
"#
        ));
    }

    for blueprint in doc
        .elements
        .iter()
        .map(crate::canvas::geometry::ElementBlueprint::from)
    {
        let stroke = &blueprint.style.stroke_color;
        let fill = blueprint.style.fill_color.as_deref().unwrap_or("none");
        let dash_attr = match blueprint.style.stroke_style {
            StrokeStyle::Solid => String::new(),
            StrokeStyle::Dashed => r#" stroke-dasharray="6,4""#.to_string(),
            StrokeStyle::Dotted => r#" stroke-dasharray="2,2""#.to_string(),
        };

        // SVG coordinates are world coordinates today: both run y-down with
        // a 1:1 unit scale, so the mapping here is the identity.
        match &blueprint.shape {
            crate::canvas::geometry::Shape::Rectangle {
                x,
                y,
                width,
                height,
                radius,
            } => {
                svg.push_str(&format!(
                    r#"<rect x="{x:.1}" y="{y:.1}" width="{width:.1}" height="{height:.1}" rx="{radius:.1}" fill="{fill}" stroke="{stroke}" stroke-width="{sw:.1}" opacity="{o:.2}"{dash_attr} />
"#,
                    sw = blueprint.style.stroke_width,
                    o = blueprint.style.opacity,
                ));
            }
            crate::canvas::geometry::Shape::Ellipse { cx, cy, rx, ry } => {
                svg.push_str(&format!(
                    r#"<ellipse cx="{cx:.1}" cy="{cy:.1}" rx="{rx:.1}" ry="{ry:.1}" fill="{fill}" stroke="{stroke}" stroke-width="{sw:.1}" opacity="{o:.2}"{dash_attr} />
"#,
                    sw = blueprint.style.stroke_width,
                    o = blueprint.style.opacity,
                ));
            }
            crate::canvas::geometry::Shape::Segment {
                start,
                end,
                arrowhead,
            } => {
                let arrow_marker = if *arrowhead {
                    format!(
                        r#" marker-end="url(#arrowhead_{})""#,
                        stroke.trim_start_matches('#')
                    )
                } else {
                    String::new()
                };
                svg.push_str(&format!(
                    r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{stroke}" stroke-width="{sw:.1}" opacity="{o:.2}"{arrow_marker}{dash} />
"#,
                    start.0, start.1, end.0, end.1,
                    sw = blueprint.style.stroke_width,
                    o = blueprint.style.opacity,
                    dash = dash_attr,
                ));
            }
            crate::canvas::geometry::Shape::Text {
                x,
                baseline_y,
                font_size,
                font_family,
                content,
                ..
            } => {
                let escaped = escape_xml(content);
                svg.push_str(&format!(
                    r#"<text x="{x:.1}" y="{baseline_y:.1}" font-family="{font_family}" font-size="{font_size:.1}" fill="{stroke}" opacity="{o:.2}">{escaped}</text>
"#,
                    o = blueprint.style.opacity,
                ));
            }
        }
    }

    svg.push_str("</svg>\n");
    svg
}

/// Single pass over the input; the flower of chained `replace` calls rebuilt
/// the string five-plus times per text element.
pub fn escape_xml(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for character in input.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::scene::CanvasElement;

    #[test]
    fn test_export_to_svg_structure() {
        let mut doc = CanvasDocument::new();
        doc.add_element(CanvasElement::new_rectangle(
            "r1", 100.0, 100.0, 150.0, 80.0, 6.0,
        ));
        doc.add_element(CanvasElement::new_arrow("a1", 250.0, 140.0, 320.0, 140.0));
        doc.add_element(CanvasElement::new_text(
            "t1",
            110.0,
            120.0,
            "Transformer Encoder",
            12.0,
        ));

        let svg = export_to_svg(&doc);
        assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
        assert!(svg.contains("<rect"));
        assert!(svg.contains("<line"));
        assert!(svg.contains("marker-end=\"url(#arrowhead_528bff)\""));
        assert!(svg.contains("Transformer Encoder"));
        assert!(svg.ends_with("</svg>\n"));
    }
}
