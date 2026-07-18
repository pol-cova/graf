//! Zero-dependency native SVG export engine (spec §M4.10).

use crate::canvas::scene::{CanvasDocument, ElementKind, StrokeStyle};

/// Exports a [`CanvasDocument`] to clean, standalone SVG markup.
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

    // Generate dynamic arrowhead markers for all stroke colors used in arrows
    let mut arrow_colors: Vec<String> = doc
        .elements
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::Arrow { .. }))
        .map(|e| e.style.stroke_color.clone())
        .collect();
    arrow_colors.sort();
    arrow_colors.dedup();
    if arrow_colors.is_empty() {
        arrow_colors.push("#528bff".to_string());
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

    for elem in &doc.elements {
        let stroke = &elem.style.stroke_color;
        let stroke_width = elem.style.stroke_width;
        let fill = elem.style.fill_color.as_deref().unwrap_or("none");
        let opacity = elem.style.opacity;

        let dash_attr = match elem.style.stroke_style {
            StrokeStyle::Solid => String::new(),
            StrokeStyle::Dashed => r#" stroke-dasharray="6,4""#.to_string(),
            StrokeStyle::Dotted => r#" stroke-dasharray="2,2""#.to_string(),
        };

        match &elem.kind {
            ElementKind::Rectangle { border_radius } => {
                svg.push_str(&format!(
                    r#"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="{:.1}" fill="{fill}" stroke="{stroke}" stroke-width="{stroke_width:.1}" opacity="{opacity:.2}"{dash_attr} />
"#,
                    elem.x, elem.y, elem.width, elem.height, border_radius
                ));
            }
            ElementKind::Ellipse => {
                let cx = elem.x + elem.width / 2.0;
                let cy = elem.y + elem.height / 2.0;
                let rx = elem.width / 2.0;
                let ry = elem.height / 2.0;
                svg.push_str(&format!(
                    r#"<ellipse cx="{cx:.1}" cy="{cy:.1}" rx="{rx:.1}" ry="{ry:.1}" fill="{fill}" stroke="{stroke}" stroke-width="{stroke_width:.1}" opacity="{opacity:.2}"{dash_attr} />
"#
                ));
            }
            ElementKind::Line {
                start_x,
                start_y,
                end_x,
                end_y,
            } => {
                svg.push_str(&format!(
                    r#"<line x1="{start_x:.1}" y1="{start_y:.1}" x2="{end_x:.1}" y2="{end_y:.1}" stroke="{stroke}" stroke-width="{stroke_width:.1}" opacity="{opacity:.2}"{dash_attr} />
"#
                ));
            }
            ElementKind::Arrow {
                start_x,
                start_y,
                end_x,
                end_y,
            } => {
                let marker_id = format!("arrowhead_{}", stroke.trim_start_matches('#'));
                svg.push_str(&format!(
                    r#"<line x1="{start_x:.1}" y1="{start_y:.1}" x2="{end_x:.1}" y2="{end_y:.1}" stroke="{stroke}" stroke-width="{stroke_width:.1}" opacity="{opacity:.2}" marker-end="url(#{marker_id})"{dash_attr} />
"#
                ));
            }
            ElementKind::Text {
                content,
                font_size,
                font_family,
            } => {
                let escaped = escape_xml(content);
                let baseline_y = elem.y + font_size;
                svg.push_str(&format!(
                    r#"<text x="{:.1}" y="{baseline_y:.1}" font-family="{font_family}" font-size="{font_size:.1}" fill="{stroke}" opacity="{opacity:.2}">{escaped}</text>
"#,
                    elem.x
                ));
            }
        }
    }

    svg.push_str("</svg>\n");
    svg
}

fn escape_xml(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
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
