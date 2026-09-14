use crate::canvas::scene::CanvasDocument;

pub fn export_to_tikz(doc: &CanvasDocument) -> String {
    let mut tikz = String::new();
    tikz.push_str("% Requires: \\usepackage{tikz}\n");
    tikz.push_str("\\begin{tikzpicture}\n");

    // SVG and TikZ disagree on axes: TikZ's y runs bottom-up, so world
    // coordinates flip sign in this emitter. The shared geometry walk
    // (canvas/geometry.rs) exposes world coords; the flip + scale are the
    // TikZ unit convention.
    for blueprint in doc
        .elements
        .iter()
        .map(crate::canvas::geometry::ElementBlueprint::from)
    {
        let stroke_color = clean_hex_color(blueprint.stroke);
        let mut draw_opts = vec![
            format!("draw={}", stroke_color),
            format!("line width={:.1}pt", blueprint.style.stroke_width),
        ];
        if let Some(filled_color) = blueprint.fill.map(clean_hex_color) {
            draw_opts.push(format!("fill={filled_color}"));
        }
        if let crate::canvas::scene::StrokeStyle::Dashed = blueprint.style.stroke_style {
            draw_opts.push("dashed".to_string());
        } else if let crate::canvas::scene::StrokeStyle::Dotted = blueprint.style.stroke_style {
            draw_opts.push("dotted".to_string());
        }
        let opts_str = draw_opts.join(", ");
        const TIKZ_PX_TO_UNIT: f32 = 0.04;
        const PT_PER_UNIT: f32 = 28.35; // TikZ lengths are set in pt

        let to = |x: f32, y: f32| (x * TIKZ_PX_TO_UNIT, -(y) * TIKZ_PX_TO_UNIT);
        let _ = PT_PER_UNIT;

        match &blueprint.shape {
            crate::canvas::geometry::Shape::Rectangle {
                x,
                y,
                width,
                height,
                radius,
            } => {
                let (x1, y1) = to(*x, *y);
                let (x2, y2) = to(*x + *width, *y + *height);

                let corners_opt = if *radius > 0.0 {
                    format!(
                        ", rounded corners={:.1}pt",
                        *radius * TIKZ_PX_TO_UNIT * PT_PER_UNIT
                    )
                } else {
                    String::new()
                };

                tikz.push_str(&format!(
                    "  \\draw[{opts_str}{corners_opt}] ({x1:.2}, {y1:.2}) rectangle ({x2:.2}, {y2:.2});\n"
                ));
            }
            crate::canvas::geometry::Shape::Ellipse { cx, cy, rx, ry } => {
                let (cx, cy) = to(*cx, *cy);
                let rx = rx * TIKZ_PX_TO_UNIT;
                let ry = ry * TIKZ_PX_TO_UNIT;
                tikz.push_str(&format!(
                    "  \\draw[{opts_str}] ({cx:.2}, {cy:.2}) ellipse ({rx:.2} and {ry:.2});\n"
                ));
            }
            crate::canvas::geometry::Shape::Segment {
                start,
                end,
                arrowhead,
            } => {
                let (x1, y1) = to(start.0, start.1);
                let (x2, y2) = to(end.0, end.1);
                let arrow_opts = if *arrowhead { "->, >=stealth, " } else { "" };
                tikz.push_str(&format!(
                    "  \\draw[{arrow_opts}{opts_str}] ({x1:.2}, {y1:.2}) -- ({x2:.2}, {y2:.2});\n"
                ));
            }
            crate::canvas::geometry::Shape::Text {
                x,
                top_y,
                font_size,
                content,
                ..
            } => {
                let (x, y) = to(*x, *top_y);
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

/// Single pass over the input; the net of chained `replace` calls marched
/// over the same bytes eight times per text element.
fn escape_latex(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for character in input.chars() {
        match character {
            '\\' => out.push_str("\\textbackslash{}"),
            '%' => out.push_str("\\%"),
            '$' => out.push_str("\\$"),
            '&' => out.push_str("\\&"),
            '#' => out.push_str("\\#"),
            '_' => out.push_str("\\_"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
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

    #[test]
    fn text_content_is_latex_escaped() {
        let mut doc = CanvasDocument::new();
        doc.add_element(CanvasElement::new_text(
            "t1",
            10.0,
            10.0,
            "Percent % Money $95 \\cmd #tag &_pair_{i}",
            12.0,
        ));

        let tikz = export_to_tikz(&doc);
        // Every LaTeX-significant character is neutralized: no raw %, $ or
        // brace pair can terminate the node body early.
        assert!(tikz.contains("\\%"), "{tikz}");
        assert!(tikz.contains("\\$95"), "{tikz}");
        assert!(tikz.contains("\\textbackslash{}cmd"), "{tikz}");
        assert!(tikz.contains("\\#tag"), "{tikz}");
        assert!(tikz.contains("\\&\\_pair\\_\\{i\\}"), "{tikz}");
    }
}
