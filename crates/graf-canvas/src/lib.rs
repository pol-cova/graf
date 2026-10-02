//! The legacy `.graf` canvas scene model, and its conversion to SVG.
//!
//! # Why this crate exists
//!
//! Graf's canvas editor was removed in #111 (see
//! `docs/adr/0001-native-swift-front-end.md`). It had no migration path, so
//! users' `.graf` files became inert: the app no longer declares the `graf`
//! document type, and nothing reads or writes the format any more.
//!
//! The format is plain JSON, so the content is fully recoverable without
//! reverse-engineering, and this crate is that recovery path. It is
//! deliberately **not** part of the app: no canvas feature is coming back,
//! and putting this behind the bridge would imply it might. It is a one-time
//! tool — build it, convert your files, move on.
//!
//! # Format
//!
//! A `.graf` file is a serialized [`CanvasDocument`]: a version, a viewport,
//! a background color, and a list of elements. Every field here is
//! transcribed from the original `src/canvas/scene.rs` so that a file
//! written by the old canvas deserializes unchanged. Two details are load
//! bearing and easy to lose:
//!
//! - `style.stroke_color` and `style.fill_color` are `Option<String>` with
//!   `#[serde(default)]`. Legacy files carry explicit hex strings; newer ones
//!   omit them and expect the kind-appropriate default. The defaults are the
//!   *effective* values used when exporting, not new UI palette choices.
//! - Geometry is in world coordinates, y-down, at a 1:1 unit scale, which is
//!   also SVG's convention. So the mapping to SVG is the identity, and the
//!   only conversion needed is text, which is stored top-left-anchored and
//!   drawn from its baseline.

use serde::{Deserialize, Serialize};

/// Default stroke for shapes. The effective fallback only; it is never
/// written into a `.graf` file, so the format stays decoupled from a palette.
pub const DEFAULT_STROKE_COLOR: &str = "#528bff";
/// Default fill for closed shapes.
pub const DEFAULT_FILL_COLOR: &str = "#21252b";
/// Default stroke for text elements (dimmer than shapes).
pub const DEFAULT_TEXT_COLOR: &str = "#abb2bf";

/// Slack around the drawing when framing the viewBox.
const PADDING: f32 = 16.0;
/// Viewport used when a document has no elements to measure.
const FALLBACK_VIEWPORT: (f32, f32, f32, f32) = (0.0, 0.0, 400.0, 300.0);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasDocument {
    pub version: u32,
    #[serde(default)]
    pub viewport: CanvasViewport,
    #[serde(default)]
    pub elements: Vec<CanvasElement>,
    #[serde(default)]
    pub background_color: Option<String>,
}

impl CanvasDocument {
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// The scene's extent in world coordinates, or `None` when it has no
    /// elements.
    pub fn bounding_box(&self) -> Option<(f32, f32, f32, f32)> {
        if self.elements.is_empty() {
            return None;
        }
        let mut bounds = self.elements[0].bounds();
        for element in &self.elements[1..] {
            let (x1, y1, x2, y2) = element.bounds();
            bounds.0 = bounds.0.min(x1);
            bounds.1 = bounds.1.min(y1);
            bounds.2 = bounds.2.max(x2);
            bounds.3 = bounds.3.max(y2);
        }
        Some(bounds)
    }
}

/// Viewport state of the editor when the file was saved. It carries no
/// visual meaning of its own — the scene is exported in world coordinates —
/// but it is part of the format, so it is kept and round-tripped.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CanvasViewport {
    pub pan_x: f32,
    pub pan_y: f32,
    pub zoom: f32,
}

impl Default for CanvasViewport {
    fn default() -> Self {
        Self {
            pan_x: 0.0,
            pan_y: 0.0,
            zoom: 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasElement {
    pub id: String,
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub y: f32,
    #[serde(default)]
    pub width: f32,
    #[serde(default)]
    pub height: f32,
    #[serde(default)]
    pub style: ElementStyle,
    pub kind: ElementKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ElementKind {
    Rectangle {
        #[serde(default)]
        border_radius: f32,
    },
    Ellipse,
    Line {
        #[serde(default)]
        start_x: f32,
        #[serde(default)]
        start_y: f32,
        #[serde(default)]
        end_x: f32,
        #[serde(default)]
        end_y: f32,
    },
    Arrow {
        #[serde(default)]
        start_x: f32,
        #[serde(default)]
        start_y: f32,
        #[serde(default)]
        end_x: f32,
        #[serde(default)]
        end_y: f32,
    },
    Text {
        #[serde(default)]
        content: String,
        #[serde(default)]
        font_size: f32,
        #[serde(default)]
        font_family: String,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrokeStyle {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElementStyle {
    /// `None` means "use the kind-appropriate default" (see
    /// [`CanvasElement::effective_stroke_color`]); legacy files carry an
    /// explicit hex string and deserialize as `Some(...)`.
    #[serde(default)]
    pub stroke_color: Option<String>,
    #[serde(default = "default_stroke_width")]
    pub stroke_width: f32,
    #[serde(default)]
    pub stroke_style: StrokeStyle,
    /// `None` means transparent for open/text elements and the shape
    /// default for closed shapes.
    #[serde(default)]
    pub fill_color: Option<String>,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
}

fn default_stroke_width() -> f32 {
    2.0
}

fn default_opacity() -> f32 {
    1.0
}

impl Default for ElementStyle {
    fn default() -> Self {
        Self {
            stroke_color: None,
            stroke_width: default_stroke_width(),
            stroke_style: StrokeStyle::Solid,
            fill_color: None,
            opacity: default_opacity(),
        }
    }
}

impl CanvasElement {
    /// Extent in world coordinates. Lines and arrows run between their
    /// endpoints, so their bounding box comes from those, not from the
    /// top-left `x`/`y`/`width`/`height` fields, which they leave zeroed.
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        match &self.kind {
            ElementKind::Line {
                start_x,
                start_y,
                end_x,
                end_y,
            }
            | ElementKind::Arrow {
                start_x,
                start_y,
                end_x,
                end_y,
            } => (
                start_x.min(*end_x),
                start_y.min(*end_y),
                start_x.max(*end_x),
                start_y.max(*end_y),
            ),
            _ => (self.x, self.y, self.x + self.width, self.y + self.height),
        }
    }

    /// Stroke used to draw/export this element, with the kind-appropriate
    /// default standing in for `None`.
    pub fn effective_stroke_color(&self) -> &str {
        let default = match &self.kind {
            ElementKind::Text { .. } => DEFAULT_TEXT_COLOR,
            _ => DEFAULT_STROKE_COLOR,
        };
        self.style.stroke_color.as_deref().unwrap_or(default)
    }

    /// Fill used to draw/export this element. Open segments and text have
    /// no fill; closed shapes fall back to [`DEFAULT_FILL_COLOR`].
    pub fn effective_fill_color(&self) -> Option<&str> {
        match &self.kind {
            ElementKind::Text { .. } | ElementKind::Line { .. } | ElementKind::Arrow { .. } => {
                self.style.fill_color.as_deref()
            }
            _ => Some(
                self.style
                    .fill_color
                    .as_deref()
                    .unwrap_or(DEFAULT_FILL_COLOR),
            ),
        }
    }
}

/// Normalized per-element geometry, ready to emit. World coordinates,
/// y-down, 1:1 — which is SVG's own convention, so nothing is transformed
/// on the way out.
#[derive(Debug, Clone, PartialEq)]
enum Shape<'a> {
    Rectangle {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radius: f32,
    },
    Ellipse {
        cx: f32,
        cy: f32,
        rx: f32,
        ry: f32,
    },
    Segment {
        start: (f32, f32),
        end: (f32, f32),
        /// Arrow, not a plain line: SVG needs its marker-end.
        arrowhead: bool,
    },
    Text {
        x: f32,
        /// SVG draws text from its baseline; `.graf` stores it top-left.
        baseline_y: f32,
        font_size: f32,
        font_family: &'a str,
        content: &'a str,
    },
}

impl<'a> From<&'a CanvasElement> for Shape<'a> {
    fn from(element: &'a CanvasElement) -> Self {
        match &element.kind {
            ElementKind::Rectangle { border_radius } => Self::Rectangle {
                x: element.x,
                y: element.y,
                width: element.width,
                height: element.height,
                radius: *border_radius,
            },
            ElementKind::Ellipse => Self::Ellipse {
                cx: element.x + element.width / 2.0,
                cy: element.y + element.height / 2.0,
                rx: element.width / 2.0,
                ry: element.height / 2.0,
            },
            ElementKind::Line {
                start_x,
                start_y,
                end_x,
                end_y,
            } => Self::Segment {
                start: (*start_x, *start_y),
                end: (*end_x, *end_y),
                arrowhead: false,
            },
            ElementKind::Arrow {
                start_x,
                start_y,
                end_x,
                end_y,
            } => Self::Segment {
                start: (*start_x, *start_y),
                end: (*end_x, *end_y),
                arrowhead: true,
            },
            ElementKind::Text {
                content,
                font_size,
                font_family,
            } => Self::Text {
                x: element.x,
                baseline_y: element.y + *font_size,
                font_size: *font_size,
                font_family,
                content,
            },
        }
    }
}

/// Renders the document as a standalone SVG.
///
/// Arrowheads need a `<marker>` per distinct stroke color, so those are
/// collected before the body is emitted.
pub fn export_to_svg(document: &CanvasDocument) -> String {
    let (min_x, min_y, max_x, max_y) = document.bounding_box().unwrap_or(FALLBACK_VIEWPORT);

    let view_x = min_x - PADDING;
    let view_y = min_y - PADDING;
    let view_width = (max_x - min_x) + PADDING * 2.0;
    let view_height = (max_y - min_y) + PADDING * 2.0;

    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{view_x:.1} {view_y:.1} {view_width:.1} {view_height:.1}\" width=\"{view_width:.1}\" height=\"{view_height:.1}\">\n"
    ));

    let mut arrow_colors: Vec<&str> = document
        .elements
        .iter()
        .filter(|element| matches!(element.kind, ElementKind::Arrow { .. }))
        .map(CanvasElement::effective_stroke_color)
        .collect();
    arrow_colors.sort_unstable();
    arrow_colors.dedup();
    if arrow_colors.is_empty() {
        arrow_colors.push(DEFAULT_STROKE_COLOR);
    }

    svg.push_str("<defs>\n");
    for color in &arrow_colors {
        let clean_id = format!("arrowhead_{}", color.trim_start_matches('#'));
        svg.push_str(&format!(
            "  <marker id=\"{clean_id}\" viewBox=\"0 0 10 10\" refX=\"8\" refY=\"5\" markerWidth=\"6\" markerHeight=\"6\" orient=\"auto-start-reverse\">\n    <path d=\"M 0 0 L 10 5 L 0 10 z\" fill=\"{color}\" />\n  </marker>\n"
        ));
    }
    svg.push_str("</defs>\n");

    if let Some(background) = &document.background_color {
        svg.push_str(&format!(
            r#"<rect x="{view_x:.1}" y="{view_y:.1}" width="{view_width:.1}" height="{view_height:.1}" fill="{background}" />
"#
        ));
    }

    for element in &document.elements {
        let stroke = element.effective_stroke_color();
        let fill = element.effective_fill_color().unwrap_or("none");
        let dash = match element.style.stroke_style {
            StrokeStyle::Solid => "",
            StrokeStyle::Dashed => r#" stroke-dasharray="6,4""#,
            StrokeStyle::Dotted => r#" stroke-dasharray="2,2""#,
        };
        let stroke_width = element.style.stroke_width;
        let opacity = element.style.opacity;

        match Shape::from(element) {
            Shape::Rectangle {
                x,
                y,
                width,
                height,
                radius,
            } => svg.push_str(&format!(
                r#"<rect x="{x:.1}" y="{y:.1}" width="{width:.1}" height="{height:.1}" rx="{radius:.1}" fill="{fill}" stroke="{stroke}" stroke-width="{stroke_width:.1}" opacity="{opacity:.2}"{dash} />
"#
            )),
            Shape::Ellipse { cx, cy, rx, ry } => svg.push_str(&format!(
                r#"<ellipse cx="{cx:.1}" cy="{cy:.1}" rx="{rx:.1}" ry="{ry:.1}" fill="{fill}" stroke="{stroke}" stroke-width="{stroke_width:.1}" opacity="{opacity:.2}"{dash} />
"#
            )),
            Shape::Segment {
                start,
                end,
                arrowhead,
            } => {
                let marker = if arrowhead {
                    format!(" marker-end=\"url(#arrowhead_{})\"", stroke.trim_start_matches('#'))
                } else {
                    String::new()
                };
                svg.push_str(&format!(
                    r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{stroke}" stroke-width="{stroke_width:.1}" opacity="{opacity:.2}"{marker}{dash} />
"#,
                    start.0, start.1, end.0, end.1,
                ));
            }
            Shape::Text {
                x,
                baseline_y,
                font_size,
                font_family,
                content,
            } => svg.push_str(&format!(
                r#"<text x="{x:.1}" y="{baseline_y:.1}" font-family="{font_family}" font-size="{font_size:.1}" fill="{stroke}" opacity="{opacity:.2}">{content}</text>
"#,
                content = escape_xml(content),
            )),
        }
    }

    svg.push_str("</svg>\n");
    svg
}

/// Escapes the five XML specials. Single pass, so already-escaped input
/// cannot be re-escaped five times over.
pub fn escape_xml(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for character in input.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(json: &str) -> CanvasDocument {
        CanvasDocument::from_json(json).expect("scene parses")
    }

    #[test]
    fn reads_a_scene_written_by_the_old_canvas() {
        // Raw string with two hashes: the fixture embeds `"#e0e0e0"`, whose
        // leading quote-hash pair would otherwise close an `r#"…"#` literal.
        let json = r##"{
            "version": 1,
            "viewport": { "pan_x": -12.5, "pan_y": 4.0, "zoom": 1.75 },
            "elements": [
                {
                    "id": "r1",
                    "x": 100.0, "y": 100.0, "width": 150.0, "height": 80.0,
                    "style": {
                        "stroke_color": "#e0e0e0",
                        "stroke_width": 2.0,
                        "stroke_style": "solid",
                        "fill_color": "#1a1a1a",
                        "opacity": 1.0
                    },
                    "kind": { "type": "rectangle", "border_radius": 6.0 }
                },
                {
                    "id": "a1",
                    "x": 0.0, "y": 0.0, "width": 0.0, "height": 0.0,
                    "style": { "stroke_width": 2.0, "stroke_style": "solid", "opacity": 1.0 },
                    "kind": { "type": "arrow", "start_x": 250.0, "start_y": 140.0, "end_x": 320.0, "end_y": 140.0 }
                }
            ]
        }"##;
        let parsed = document(json);
        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.viewport.zoom, 1.75);
        assert_eq!(parsed.elements.len(), 2);
        assert_eq!(
            parsed.elements[0].style.stroke_color.as_deref(),
            Some("#e0e0e0")
        );
        assert_eq!(parsed.elements[0].style.stroke_style, StrokeStyle::Solid);
        // The arrow carries no explicit colors, so the defaults must apply.
        assert_eq!(
            parsed.elements[1].effective_stroke_color(),
            DEFAULT_STROKE_COLOR
        );
        assert_eq!(parsed.elements[1].effective_fill_color(), None);
    }

    #[test]
    fn missing_optional_fields_take_defaults() {
        let parsed = document(
            r#"{ "version": 1, "elements": [
            { "id": "e", "kind": { "type": "rectangle" } }
        ] }"#,
        );
        let element = &parsed.elements[0];
        assert_eq!(element.style, ElementStyle::default());
        assert_eq!(element.style.stroke_width, 2.0);
        assert_eq!(element.style.opacity, 1.0);
        // A closed shape falls back to the shape fill default.
        assert_eq!(element.effective_fill_color(), Some(DEFAULT_FILL_COLOR));
    }

    #[test]
    fn text_takes_the_dimmer_default_stroke() {
        let parsed = document(
            r#"{ "version": 1, "elements": [
            { "id": "t", "kind": { "type": "text", "content": "x", "font_size": 12.0, "font_family": "Menlo" } }
        ] }"#,
        );
        assert_eq!(
            parsed.elements[0].effective_stroke_color(),
            DEFAULT_TEXT_COLOR
        );
        assert_eq!(parsed.elements[0].effective_fill_color(), None);
    }

    #[test]
    fn bounding_box_uses_line_endpoints_not_the_zeroed_rect() {
        let parsed = document(
            r#"{ "version": 1, "elements": [
            { "id": "a", "x": 0.0, "y": 0.0, "width": 0.0, "height": 0.0,
              "style": {}, "kind": { "type": "arrow", "start_x": 30.0, "start_y": 10.0, "end_x": 5.0, "end_y": 40.0 } }
        ] }"#,
        );
        assert_eq!(parsed.bounding_box(), Some((5.0, 10.0, 30.0, 40.0)));
    }

    #[test]
    fn bounding_box_of_an_empty_document_is_none() {
        let parsed = document(r#"{ "version": 1, "elements": [] }"#);
        assert_eq!(parsed.bounding_box(), None);
    }

    #[test]
    fn exports_every_shape_kind() {
        let parsed = document(
            r#"{ "version": 1, "elements": [
            { "id": "r", "x": 10.0, "y": 10.0, "width": 40.0, "height": 20.0,
              "style": {}, "kind": { "type": "rectangle", "border_radius": 4.0 } },
            { "id": "e", "x": 10.0, "y": 10.0, "width": 40.0, "height": 20.0,
              "style": {}, "kind": { "type": "ellipse" } },
            { "id": "l", "style": {},
              "kind": { "type": "line", "start_x": 0.0, "start_y": 0.0, "end_x": 10.0, "end_y": 10.0 } },
            { "id": "a", "style": {},
              "kind": { "type": "arrow", "start_x": 0.0, "start_y": 0.0, "end_x": 10.0, "end_y": 0.0 } },
            { "id": "t", "x": 5.0, "y": 5.0, "style": {},
              "kind": { "type": "text", "content": "Label", "font_size": 12.0, "font_family": "Menlo" } }
        ] }"#,
        );
        let svg = export_to_svg(&parsed);
        assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
        assert!(svg.contains("<rect"));
        assert!(svg.contains("<ellipse"));
        // The plain line has no marker; only the arrow does.
        assert_eq!(
            svg.matches("marker-end=\"url(#arrowhead_528bff)\"").count(),
            1
        );
        assert!(svg.contains("<text"));
        assert!(svg.contains("Label"));
        assert!(svg.ends_with("</svg>\n"));
    }

    #[test]
    fn text_is_anchored_from_its_baseline() {
        let parsed = document(
            r#"{ "version": 1, "elements": [
            { "id": "t", "x": 7.0, "y": 20.0, "style": {},
              "kind": { "type": "text", "content": "Hi", "font_size": 12.0, "font_family": "Menlo" } }
        ] }"#,
        );
        let svg = export_to_svg(&parsed);
        // y is stored top-left; SVG draws from the baseline, so 20 + 12.
        assert!(svg.contains("<text x=\"7.0\" y=\"32.0\""));
    }

    #[test]
    fn stroke_styles_become_dash_arrays() {
        // Raw string: `#` inside starts no interpolation but does open a
        // nested hash, so a `"#rrggbb"` literal must use `r#"…"#`.
        let parsed = document(
            r##"{ "version": 1, "elements": [
            { "id": "d", "style": { "stroke_style": "dashed", "stroke_width": 1.0, "opacity": 1.0 },
              "kind": { "type": "line", "start_x": 0.0, "start_y": 0.0, "end_x": 1.0, "end_y": 1.0 } },
            { "id": "o", "style": { "stroke_style": "dotted", "stroke_width": 1.0, "opacity": 1.0 },
              "kind": { "type": "line", "start_x": 0.0, "start_y": 0.0, "end_x": 1.0, "end_y": 1.0 } }
        ] }"##,
        );
        let svg = export_to_svg(&parsed);
        assert_eq!(svg.matches("stroke-dasharray=\"6,4\"").count(), 1);
        assert_eq!(svg.matches("stroke-dasharray=\"2,2\"").count(), 1);
    }

    #[test]
    fn one_arrowhead_marker_per_distinct_color() {
        let parsed = document(
            r##"{ "version": 1, "elements": [
            { "id": "a1", "style": { "stroke_color": "#ff0000" },
              "kind": { "type": "arrow", "start_x": 0.0, "start_y": 0.0, "end_x": 1.0, "end_y": 0.0 } },
            { "id": "a2", "style": { "stroke_color": "#ff0000" },
              "kind": { "type": "arrow", "start_x": 0.0, "start_y": 1.0, "end_x": 1.0, "end_y": 1.0 } },
            { "id": "a3", "style": { "stroke_color": "#00ff00" },
              "kind": { "type": "arrow", "start_x": 0.0, "start_y": 2.0, "end_x": 1.0, "end_y": 2.0 } }
        ] }"##,
        );
        let svg = export_to_svg(&parsed);
        assert_eq!(svg.matches("<marker id=\"arrowhead_ff0000\"").count(), 1);
        assert_eq!(svg.matches("<marker id=\"arrowhead_00ff00\"").count(), 1);
    }

    #[test]
    fn background_is_emitted_behind_the_shapes() {
        let parsed = document(
            r##"{ "version": 1, "background_color": "#101014", "elements": [
                { "id": "r", "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0,
                  "style": {}, "kind": { "type": "rectangle" } }
            ] }"##,
        );
        let svg = export_to_svg(&parsed);
        let background = svg.find("fill=\"#101014\"").expect("background rect");
        let shape = svg.find("<rect x=\"0.0\"").expect("element rect");
        assert!(background < shape, "background must be painted first");
    }

    #[test]
    fn text_content_is_xml_escaped() {
        // `r##` because the fixture contains `(\"x\")` — a quote-hash pair is
        // not present, but the doubled hash keeps this robust if it grows one.
        let parsed = document(
            r##"{
            "version": 1,
            "elements": [{
                "id": "t", "x": 0.0, "y": 0.0, "style": {},
                "kind": { "type": "text", "content": "<script>alert(\"x\") & 'tag'</script>",
                          "font_size": 12.0, "font_family": "Menlo" }
            }]
        }"##,
        );
        let svg = export_to_svg(&parsed);
        // No raw markup reaches the document body...
        assert!(!svg.contains("<script"));
        assert!(!svg.contains("alert(\"x\")"));
        // ...and every special is escaped exactly once.
        assert!(
            svg.contains("&lt;script&gt;alert(&quot;x&quot;) &amp; &apos;tag&apos;&lt;/script&gt;")
        );
    }

    #[test]
    fn escape_xml_handles_all_specials_once() {
        assert_eq!(
            escape_xml(r#"a & b < c > d " e ' f"#),
            "a &amp; b &lt; c &gt; d &quot; e &apos; f"
        );
        // An already-escaped entity must not be re-escaped.
        assert_eq!(escape_xml("&amp;"), "&amp;amp;");
        assert_eq!(escape_xml("plain"), "plain");
    }

    #[test]
    fn empty_document_still_produces_a_valid_svg() {
        let parsed = document(r#"{ "version": 1, "elements": [] }"#);
        let svg = export_to_svg(&parsed);
        assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
        // Falls back to a 400x300 viewport plus padding on each side.
        assert!(svg.contains("viewBox=\"-16.0 -16.0 432.0 332.0\""));
        assert!(svg.ends_with("</svg>\n"));
    }

    #[test]
    fn malformed_json_is_an_error_not_a_panic() {
        assert!(CanvasDocument::from_json("{ not json").is_err());
        assert!(
            CanvasDocument::from_json("[]").is_err(),
            "array is not a document"
        );
    }
}
