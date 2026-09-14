use serde::{Deserialize, Serialize};

/// Default stroke for shapes. Kept as the effective fallback only; it is
/// never serialized, so `.graf` files stay decoupled from the palette.
pub const DEFAULT_STROKE_COLOR: &str = "#528bff";
/// Default fill for closed shapes.
pub const DEFAULT_FILL_COLOR: &str = "#21252b";
/// Default stroke for text elements (dimmer than shapes).
pub const DEFAULT_TEXT_COLOR: &str = "#abb2bf";

/// On-screen slack (px) around a clicked point that still counts as a hit.
pub(crate) const HIT_TEST_TOLERANCE_PX: f32 = 6.0;

/// Memoized scene bounding box: the inner value is the `(min_x, min_y,
/// max_x, max_y)` result, the outer `None` means "not computed yet".
pub(crate) type BoundingBoxCache = std::cell::Cell<Option<Option<(f32, f32, f32, f32)>>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasDocument {
    pub version: u32,
    pub viewport: CanvasViewport,
    pub elements: Vec<CanvasElement>,
    pub background_color: Option<String>,
    /// Memoized `bounding_box` result, cleared by every mutator. `None`
    /// means "stale", so an empty-document result is cached too.
    #[serde(skip)]
    bounding_box_cache: BoundingBoxCache,
}

/// Hand-written so document equality ignores the memoization cache: two
/// snapshots with the same elements but different cache states must be
/// equal, or undo/serde comparisons would depend on incidental geometry
/// dereferences.
impl PartialEq for CanvasDocument {
    fn eq(&self, other: &Self) -> bool {
        self.version == other.version
            && self.viewport == other.viewport
            && self.elements == other.elements
            && self.background_color == other.background_color
    }
}

impl Default for CanvasDocument {
    fn default() -> Self {
        Self::new()
    }
}

impl CanvasDocument {
    pub fn new() -> Self {
        Self {
            version: 1,
            viewport: CanvasViewport::default(),
            elements: Vec::new(),
            background_color: None,
            bounding_box_cache: std::cell::Cell::new(None),
        }
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn add_element(&mut self, element: CanvasElement) {
        self.elements.push(element);
        self.bounding_box_cache.set(None);
    }

    pub fn remove_element(&mut self, id: &str) -> Option<CanvasElement> {
        if let Some(pos) = self.elements.iter().position(|e| e.id == id) {
            self.bounding_box_cache.set(None);
            Some(self.elements.remove(pos))
        } else {
            None
        }
    }

    /// Clears cached geometry for direct element edits that bypass the
    /// `add`/`remove` mutators (e.g. interactive dragging).
    pub fn invalidate_geometry_cache(&mut self) {
        self.bounding_box_cache.set(None);
    }

    pub fn bounding_box(&self) -> Option<(f32, f32, f32, f32)> {
        if let Some(cached) = self.bounding_box_cache.get() {
            return cached;
        }

        let result = self.compute_bounding_box();
        self.bounding_box_cache.set(Some(result));
        result
    }

    fn compute_bounding_box(&self) -> Option<(f32, f32, f32, f32)> {
        if self.elements.is_empty() {
            return None;
        }

        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;

        for elem in &self.elements {
            let (ex1, ey1, ex2, ey2) = elem.bounds();

            min_x = min_x.min(ex1);
            min_y = min_y.min(ey1);
            max_x = max_x.max(ex2);
            max_y = max_y.max(ey2);
        }

        Some((min_x, min_y, max_x, max_y))
    }

    /// Zoom-scaled hit test: `zoom` is the viewport zoom, so the click
    /// tolerance stays `HIT_TEST_TOLERANCE_PX` screen pixels at any zoom.
    pub fn find_element_at(&self, x: f32, y: f32, zoom: f32) -> Option<&CanvasElement> {
        let tolerance = HIT_TEST_TOLERANCE_PX / zoom.max(f32::EPSILON);
        self.elements
            .iter()
            .rev()
            .find(|e| e.contains_point(x, y, tolerance))
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct CanvasViewport {
    pub pan_x: f32,
    pub pan_y: f32,
    pub zoom: f32,
}

impl CanvasViewport {
    /// Single source of truth for the world↔screen mapping used by both
    /// rendering and input; pan and zoom must stay consistent across the two
    /// or clicks land away from shapes.
    pub fn world_to_screen(&self, x: f32, y: f32) -> (f32, f32) {
        ((x - self.pan_x) * self.zoom, (y - self.pan_y) * self.zoom)
    }

    pub fn screen_to_world(&self, x: f32, y: f32) -> (f32, f32) {
        (x / self.zoom + self.pan_x, y / self.zoom + self.pan_y)
    }
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CanvasElement {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub style: ElementStyle,
    pub kind: ElementKind,
}

impl CanvasElement {
    pub fn new_rectangle(
        id: impl Into<String>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        border_radius: f32,
    ) -> Self {
        Self {
            id: id.into(),
            x,
            y,
            width,
            height,
            style: ElementStyle::default(),
            kind: ElementKind::Rectangle { border_radius },
        }
    }

    pub fn new_ellipse(id: impl Into<String>, x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            id: id.into(),
            x,
            y,
            width,
            height,
            style: ElementStyle::default(),
            kind: ElementKind::Ellipse,
        }
    }

    pub fn new_arrow(
        id: impl Into<String>,
        start_x: f32,
        start_y: f32,
        end_x: f32,
        end_y: f32,
    ) -> Self {
        Self {
            id: id.into(),
            x: start_x.min(end_x),
            y: start_y.min(end_y),
            width: (end_x - start_x).abs(),
            height: (end_y - start_y).abs(),
            style: ElementStyle::default(),
            kind: ElementKind::Arrow {
                start_x,
                start_y,
                end_x,
                end_y,
            },
        }
    }

    pub fn new_line(
        id: impl Into<String>,
        start_x: f32,
        start_y: f32,
        end_x: f32,
        end_y: f32,
    ) -> Self {
        Self {
            id: id.into(),
            x: start_x.min(end_x),
            y: start_y.min(end_y),
            width: (end_x - start_x).abs(),
            height: (end_y - start_y).abs(),
            style: ElementStyle::default(),
            kind: ElementKind::Line {
                start_x,
                start_y,
                end_x,
                end_y,
            },
        }
    }

    pub fn new_text(
        id: impl Into<String>,
        x: f32,
        y: f32,
        content: impl Into<String>,
        font_size: f32,
    ) -> Self {
        let content_str = content.into();
        let width = (content_str.chars().count() as f32 * font_size * 0.6).max(20.0);
        let height = font_size * 1.4;

        Self {
            id: id.into(),
            x,
            y,
            width,
            height,
            style: ElementStyle {
                stroke_width: 1.0,
                ..ElementStyle::default()
            },
            kind: ElementKind::Text {
                content: content_str,
                font_size,
                font_family: "system-ui".to_string(),
            },
        }
    }

    /// Top-left/size axis-aligned bounds of the element in world
    /// coordinates. Single source of truth, shared by bounding-box
    /// computation, render culling, and the exporters.
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
    /// no fill; closed shapes fall back to `DEFAULT_FILL_COLOR`.
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

    pub fn contains_point(&self, px: f32, py: f32, tolerance: f32) -> bool {
        match &self.kind {
            ElementKind::Ellipse => {
                let rx = self.width / 2.0;
                let ry = self.height / 2.0;
                if rx <= 0.0 || ry <= 0.0 {
                    return false;
                }
                let cx = self.x + rx;
                let cy = self.y + ry;
                let dx = (px - cx) / rx;
                let dy = (py - cy) / ry;
                (dx * dx + dy * dy) <= 1.0
            }
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
            } => {
                let vx = end_x - start_x;
                let vy = end_y - start_y;
                let len_sq = vx * vx + vy * vy;
                let tolerance_sq = tolerance * tolerance;
                if len_sq < 0.001 {
                    let dx = px - start_x;
                    let dy = py - start_y;
                    return (dx * dx + dy * dy) <= tolerance_sq;
                }
                let t = (((px - start_x) * vx + (py - start_y) * vy) / len_sq).clamp(0.0, 1.0);
                let qx = start_x + t * vx;
                let qy = start_y + t * vy;
                let dx = px - qx;
                let dy = py - qy;
                (dx * dx + dy * dy) <= tolerance_sq
            }
            _ => {
                px >= self.x
                    && px <= self.x + self.width
                    && py >= self.y
                    && py <= self.y + self.height
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ElementKind {
    Rectangle {
        border_radius: f32,
    },
    Ellipse,
    Line {
        start_x: f32,
        start_y: f32,
        end_x: f32,
        end_y: f32,
    },
    Arrow {
        start_x: f32,
        start_y: f32,
        end_x: f32,
        end_y: f32,
    },
    Text {
        content: String,
        font_size: f32,
        font_family: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ElementStyle {
    /// `None` means "use the kind-appropriate default" (see
    /// [`CanvasElement::effective_stroke_color`]); legacy files carry an
    /// explicit hex string and deserialize as `Some(...)`.
    #[serde(default)]
    pub stroke_color: Option<String>,
    pub stroke_width: f32,
    pub stroke_style: StrokeStyle,
    /// `None` means transparent for open/text elements and the shape
    /// default for closed shapes.
    #[serde(default)]
    pub fill_color: Option<String>,
    pub opacity: f32,
}

impl Default for ElementStyle {
    fn default() -> Self {
        Self {
            stroke_color: None,
            stroke_width: 2.0,
            stroke_style: StrokeStyle::Solid,
            fill_color: None,
            opacity: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum StrokeStyle {
    Solid,
    Dashed,
    Dotted,
}

#[cfg(test)]
mod tests_extra {
    use super::*;

    #[test]
    fn bounding_box_cache_stays_consistent_through_mutators() {
        let mut doc = CanvasDocument::new();
        assert!(doc.bounding_box().is_none());

        doc.add_element(CanvasElement::new_rectangle(
            "r1", 0.0, 0.0, 10.0, 10.0, 0.0,
        ));
        assert_eq!(doc.bounding_box(), Some((0.0, 0.0, 10.0, 10.0)));
        // Cached until invalidated.
        assert_eq!(doc.bounding_box(), Some((0.0, 0.0, 10.0, 10.0)));

        // Direct (drag-style) mutation bypasses mutators; explicit invalidation.
        doc.elements[0].x = 100.0;
        doc.invalidate_geometry_cache();
        assert_eq!(doc.bounding_box(), Some((100.0, 0.0, 110.0, 10.0)));

        doc.remove_element("r1");
        assert!(doc.bounding_box().is_none());

        doc.add_element(CanvasElement::new_arrow("a1", 5.0, 5.0, 50.0, 40.0));
        assert_eq!(doc.bounding_box(), Some((5.0, 5.0, 50.0, 40.0)));
    }

    #[test]
    fn surviving_serialization_roundtrip_recomputes_bbox() {
        let mut doc = CanvasDocument::new();
        doc.add_element(CanvasElement::new_rectangle(
            "r1", 3.0, 4.0, 50.0, 20.0, 2.0,
        ));
        let json = doc.to_json().unwrap();
        let restored = CanvasDocument::from_json(&json).unwrap();
        assert_eq!(restored.bounding_box(), Some((3.0, 4.0, 53.0, 24.0)));
    }

    #[test]
    fn default_colors_are_not_serialized_into_the_document() {
        let mut doc = CanvasDocument::new();
        doc.add_element(CanvasElement::new_rectangle(
            "r1", 0.0, 0.0, 10.0, 10.0, 0.0,
        ));
        doc.add_element(CanvasElement::new_text("t1", 0.0, 0.0, "hi", 12.0));

        let json = doc.to_json().unwrap();
        // Theme hexes must not leak into the file: defaults stay in code.
        for hex in [DEFAULT_STROKE_COLOR, DEFAULT_FILL_COLOR, DEFAULT_TEXT_COLOR] {
            assert!(!json.contains(hex), "{hex} serialized");
        }

        // Round-trip keeps the effective fallbacks intact.
        let loaded = CanvasDocument::from_json(&json).unwrap();
        assert_eq!(
            loaded.elements[0].effective_stroke_color(),
            DEFAULT_STROKE_COLOR
        );
        assert_eq!(
            loaded.elements[0].effective_fill_color(),
            Some(DEFAULT_FILL_COLOR)
        );
        assert_eq!(
            loaded.elements[1].effective_stroke_color(),
            DEFAULT_TEXT_COLOR
        );
        assert_eq!(loaded.elements[1].effective_fill_color(), None);
    }

    #[test]
    fn legacy_graf_files_with_explicit_hexes_round_trip() {
        // Shape file written by an earlier version: explicit theme colors.
        let legacy = r##"{
            "version": 1,
            "viewport": {"pan_x": 0.0, "pan_y": 0.0, "zoom": 1.0},
            "elements": [{
                "id": "r1", "x": 10.0, "y": 10.0, "width": 40.0, "height": 30.0,
                "style": {
                    "stroke_color": "#ff00ff", "stroke_width": 2.0,
                    "stroke_style": "Solid", "fill_color": "#123456", "opacity": 1.0
                },
                "kind": {"Rectangle": {"border_radius": 0.0}}
            }],
            "grid_enabled": true
        }"##;
        let doc = CanvasDocument::from_json(legacy).expect("legacy file must parse");

        // `grid_enabled` no longer exists; legacy files must still load.
        assert_eq!(doc.elements.len(), 1);
        assert_eq!(doc.elements[0].effective_stroke_color(), "#ff00ff");
        assert_eq!(doc.elements[0].effective_fill_color(), Some("#123456"));

        // Re-saving drops the retired field and explicit defaults, but keeps
        // user-authored colors.
        let json = doc.to_json().unwrap();
        assert!(!json.contains("grid_enabled"));
        assert!(json.contains("#ff00ff"));
        assert_eq!(doc, CanvasDocument::from_json(&json).unwrap());
    }

    #[test]
    fn equality_ignores_the_bounding_box_cache() {
        let mut a = CanvasDocument::new();
        a.add_element(CanvasElement::new_rectangle(
            "r1", 0.0, 0.0, 10.0, 10.0, 0.0,
        ));
        let b = a.clone();

        // Warming the cache on one side must not break equality.
        a.bounding_box();
        assert!(a.bounding_box_cache.get().is_some());
        assert!(b.bounding_box_cache.get().is_none());
        assert_eq!(a, b);
    }

    #[test]
    fn text_width_counts_characters_not_bytes() {
        let ascii = CanvasElement::new_text("a", 0.0, 0.0, "abcdef", 10.0);
        let multibyte = CanvasElement::new_text("b", 0.0, 0.0, "αβγδ", 10.0);
        // "αβγδ" is 8 bytes but 4 characters.
        let ascii_len = "abcdef".len();
        assert_eq!(multibyte.width, ascii.width * 4.0 / ascii_len as f32);
        assert_eq!(multibyte.width, ascii.width * 2.0 / 3.0);
    }

    #[test]
    fn hit_test_tolerance_magnifies_with_zoom() {
        let mut doc = CanvasDocument::new();
        doc.add_element(CanvasElement::new_arrow("a1", 0.0, 0.0, 100.0, 0.0));

        // Same screen-click tolerance in world units means a hit that misses
        // at 3 px off the line at zoom 1 must still hit at zoom 8 within the
        // shrunken world tolerance.
        assert!(doc.find_element_at(50.0, 5.0, 1.0).is_some());
        assert!(doc.find_element_at(50.0, 5.0, 8.0).is_none());
        assert!(doc.find_element_at(50.0, 0.5, 8.0).is_some());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canvas_document_serialization() {
        let mut doc = CanvasDocument::new();
        doc.add_element(CanvasElement::new_rectangle(
            "rect-1", 50.0, 50.0, 120.0, 80.0, 4.0,
        ));
        doc.add_element(CanvasElement::new_text(
            "text-1",
            60.0,
            70.0,
            "Architecture",
            14.0,
        ));

        let json = doc.to_json().expect("Serialization failed");
        assert!(json.contains("rect-1"));
        assert!(json.contains("Architecture"));

        let loaded = CanvasDocument::from_json(&json).expect("Deserialization failed");
        assert_eq!(loaded.elements.len(), 2);
        assert_eq!(loaded.elements[0].id, "rect-1");
        assert_eq!(loaded.elements[1].id, "text-1");
    }

    #[test]
    fn test_bounding_box_and_hit_testing() {
        let mut doc = CanvasDocument::new();
        let rect = CanvasElement::new_rectangle("r1", 10.0, 20.0, 100.0, 50.0, 0.0);
        let arrow = CanvasElement::new_arrow("a1", 110.0, 45.0, 200.0, 45.0);

        doc.add_element(rect);
        doc.add_element(arrow);

        let (min_x, min_y, max_x, max_y) = doc.bounding_box().unwrap();
        assert_eq!(min_x, 10.0);
        assert_eq!(min_y, 20.0);
        assert_eq!(max_x, 200.0);
        assert_eq!(max_y, 70.0);

        assert!(doc.find_element_at(50.0, 40.0, 1.0).is_some());
        assert_eq!(doc.find_element_at(50.0, 40.0, 1.0).unwrap().id, "r1");
        assert!(doc.find_element_at(500.0, 500.0, 1.0).is_none());
    }
}
