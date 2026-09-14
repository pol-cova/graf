//! Shared element decomposition for the SVG and TikZ exporters.
//!
//! Both emitters walk the same document, but they had drifted: duplicated
//! geometry math, and text baselines/unit scales justified nowhere. This
//! module is the single source of truth for per-element geometry:
//!
//! - Geometry is emitted in *world coordinates* (the `.graf` scene space).
//! - Axis conventions belong to the emitters. SVG is y-down; TikZ flips y
//!   (its baseline runs bottom-up) and applies a unit scale, so both
//!   conventions are named in the emitters.
//! - Text anchors differ by design: SVG draws from the baseline (hence the
//!   `baseline_y = y + font_size` conversion from top-left semantics);
//!   TikZ anchors the glyph box at its top-left, using the raw `y`.

use crate::canvas::scene::{CanvasElement, ElementStyle};

/// Normalized per-element geometry in world coordinates.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Shape<'a> {
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
        /// Arrow, not a plain line: SVG needs its marker-end, TikZ needs
        /// `->`. Arrowheads are emitter syntax, the direction is geometry.
        arrowhead: bool,
    },
    Text {
        x: f32,
        top_y: f32,
        baseline_y: f32,
        font_size: f32,
        font_family: &'a str,
        content: &'a str,
    },
}

/// One element walked once, ready for either emitter to render.
#[derive(Debug, Clone)]
pub(crate) struct ElementBlueprint<'a> {
    /// The element's id, exposed for emitters that key per-shape resources
    /// (e.g. SVG's arrow markers); geometry consumers share the style.
    #[allow(dead_code)] // one of the emitter pair may not need it
    pub id: &'a str,
    pub style: &'a ElementStyle,
    pub shape: Shape<'a>,
}

impl<'a> From<&'a CanvasElement> for ElementBlueprint<'a> {
    fn from(elem: &'a CanvasElement) -> Self {
        let shape = match &elem.kind {
            crate::canvas::scene::ElementKind::Rectangle { border_radius } => Shape::Rectangle {
                x: elem.x,
                y: elem.y,
                width: elem.width,
                height: elem.height,
                radius: *border_radius,
            },
            crate::canvas::scene::ElementKind::Ellipse => Shape::Ellipse {
                cx: elem.x + elem.width / 2.0,
                cy: elem.y + elem.height / 2.0,
                rx: elem.width / 2.0,
                ry: elem.height / 2.0,
            },
            crate::canvas::scene::ElementKind::Line {
                start_x,
                start_y,
                end_x,
                end_y,
            } => Shape::Segment {
                start: (*start_x, *start_y),
                end: (*end_x, *end_y),
                arrowhead: false,
            },
            crate::canvas::scene::ElementKind::Arrow {
                start_x,
                start_y,
                end_x,
                end_y,
            } => Shape::Segment {
                start: (*start_x, *start_y),
                end: (*end_x, *end_y),
                arrowhead: true,
            },
            crate::canvas::scene::ElementKind::Text {
                content,
                font_size,
                font_family,
            } => Shape::Text {
                x: elem.x,
                top_y: elem.y,
                baseline_y: elem.y + *font_size,
                font_size: *font_size,
                font_family,
                content,
            },
        };
        ElementBlueprint {
            id: &elem.id,
            style: &elem.style,
            shape,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::scene::{CanvasElement, ElementKind, ElementStyle};

    #[test]
    fn ellipse_geometry_is_center_and_radii() {
        let elem = CanvasElement::new_ellipse("e1", 10.0, 20.0, 60.0, 40.0);
        let blueprint = ElementBlueprint::from(&elem);
        match blueprint.shape {
            Shape::Ellipse { cx, cy, rx, ry } => {
                assert_eq!(cx, 40.0);
                assert_eq!(cy, 40.0);
                assert_eq!(rx, 30.0);
                assert_eq!(ry, 20.0);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn text_baseline_is_top_plus_font_size() {
        let elem = CanvasElement::new_text("t1", 5.0, 30.0, "hi", 12.0);
        let blueprint = ElementBlueprint::from(&elem);
        match blueprint.shape {
            Shape::Text {
                top_y, baseline_y, ..
            } => {
                assert_eq!(top_y, 30.0);
                assert_eq!(baseline_y, 42.0);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn style_is_borrowed_not_copied_per_emitter() {
        let mut elem = CanvasElement::new_rectangle("r1", 0.0, 0.0, 1.0, 1.0, 0.0);
        elem.style.fill_color = Some("#123456".to_string());
        let blueprint = ElementBlueprint::from(&elem);
        assert_eq!(blueprint.style.fill_color.as_deref(), Some("#123456"));
        assert!(matches!(blueprint.shape, Shape::Rectangle { .. }));
        let _ = ElementStyle::default();
        let _ = ElementKind::Ellipse;
    }
}
