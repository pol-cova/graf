//! Zero-dependency native TikZ LaTeX export engine.

use crate::canvas::scene::{CanvasDocument, ElementKind, StrokeStyle};

/// Exports a [`CanvasDocument`] to clean, standalone LaTeX TikZ markup.
pub fn export_to_tikz(doc: &CanvasDocument) -> String {
    let scale = 0.04; // 1 pixel = 0.04 cm in TikZ coordinate space

    let mut tikz = String::new();
    tikz.push_str("% Requires: \\usepackage{tikz}\n");
    tikz.push_str("\\begin{tikzpicture}\n");

    for elem in &doc.elements {
        let stroke_color = clean_hex_color(&elem.style.stroke_color);
        let stroke_width = elem.style.stroke_width;
        let fill_color = elem.style.fill_color.as_deref().map(clean_hex_color);

        let mut draw_opts = Vec::new();
        draw_opts.push(format!("draw={stroke_color}"));
        draw_opts.push(format!("line width={stroke_width:.1}pt"));

        if let Some(fill) = fill_color {
            draw_opts.push(format!("fill={fill}"));
        }

        match elem.style.stroke_style {
            StrokeStyle::Solid => {}
            StrokeStyle::Dashed => draw_opts.push("dashed".to_string()),
            StrokeStyle::Dotted => draw_opts.push("dotted".to_string()),
        }

        let opts_str = draw_opts.join(", ");

        match &elem.kind {
            ElementKind::Rectangle { border_radius } => {
                let x1 = elem.x * scale;
                let y1 = -elem.y * scale;
                let x2 = (elem.x + elem.width) * scale;
                let y2 = -(elem.y + elem.height) * scale;

                let corners_opt = if *border_radius > 0.0 {
                    format!(", rounded corners={:.1}pt", border_radius * scale * 28.35)
                } else {
                    String::new()
                };

                tikz.push_str(&format!(
                    "  \\draw[{opts_str}{corners_opt}] ({x1:.2}, {y1:.2}) rectangle ({x2:.2}, {y2:.2});\n"
                ));
            }
            ElementKind::Ellipse => {
                let rx = (elem.width / 2.0) * scale;
                let ry = (elem.height / 2.0) * scale;
                let cx = (elem.x + elem.width / 2.0) * scale;
                let cy = -(elem.y + elem.height / 2.0) * scale;

                tikz.push_str(&format!(
                    "  \\draw[{opts_str}] ({cx:.2}, {cy:.2}) ellipse ({rx:.2} and {ry:.2});\n"
                ));
            }
            ElementKind::Line {
                start_x,
                start_y,
                end_x,
                end_y,
            } => {
                let x1 = start_x * scale;
                let y1 = -start_y * scale;
                let x2 = end_x * scale;
                let y2 = -end_y * scale;

                tikz.push_str(&format!(
                    "  \\draw[{opts_str}] ({x1:.2}, {y1:.2}) -- ({x2:.2}, {y2:.2});\n"
                ));
            }
            ElementKind::Arrow {
                start_x,
                start_y,
                end_x,
                end_y,
            } => {
                let x1 = start_x * scale;
                let y1 = -start_y * scale;
                let x2 = end_x * scale;
                let y2 = -end_y * scale;

                tikz.push_str(&format!(
                    "  \\draw[->, >=stealth, {opts_str}] ({x1:.2}, {y1:.2}) -- ({x2:.2}, {y2:.2});\n"
                ));
            }
            ElementKind::Text {
                content, font_size, ..
            } => {
                let x = elem.x * scale;
                let y = -elem.y * scale;
                let escaped = escape_latex(content);

                tikz.push_str(&format!(
                    "  \\node[anchor=north west, text={stroke_color}, font=\\fontsize{{{font_size:.0}}}{{{font_size:.0}}}\\selectfont] at ({x:.2}, {y:.2}) {{{escaped}}};\n"
                ));
            }
        }
    }

    tikz.push_str("\\end{tikzpicture}\n");
    tikz
}

fn clean_hex_color(hex: &str) -> String {
    let clean = hex.trim_start_matches('#');
    if clean.eq_ignore_ascii_case("ffffff") || clean.eq_ignore_ascii_case("fff") {
        "white".to_string()
    } else if clean.eq_ignore_ascii_case("000000") || clean.eq_ignore_ascii_case("000") {
        "black".to_string()
    } else {
        format!("{{HTML}}{{{clean}}}")
    }
}

fn escape_latex(input: &str) -> String {
    input
        .replace('\\', "\\textbackslash{}")
        .replace('%', "\\%")
        .replace('$', "\\$")
        .replace('&', "\\&")
        .replace('#', "\\#")
        .replace('_', "\\_")
        .replace('{', "\\{")
        .replace('}', "\\}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::scene::CanvasElement;

    #[test]
    fn test_export_to_tikz_structure() {
        let mut doc = CanvasDocument::new();
        doc.add_element(CanvasElement::new_rectangle(
            "r1", 100.0, 100.0, 150.0, 80.0, 6.0,
        ));
        doc.add_element(CanvasElement::new_arrow("a1", 250.0, 140.0, 320.0, 140.0));
        doc.add_element(CanvasElement::new_text(
            "t1",
            110.0,
            120.0,
            "Neural Architecture",
            12.0,
        ));

        let tikz = export_to_tikz(&doc);
        assert!(tikz.starts_with("% Requires: \\usepackage{tikz}\n\\begin{tikzpicture}"));
        assert!(tikz.contains("\\draw["));
        assert!(tikz.contains("rectangle"));
        assert!(tikz.contains("->, >=stealth"));
        assert!(tikz.contains("Neural Architecture"));
        assert!(tikz.ends_with("\\end{tikzpicture}\n"));
    }
}
