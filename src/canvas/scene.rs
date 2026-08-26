use serde::{Deserialize, Serialize};

pub const DEFAULT_STROKE_COLOR: &str = "#528bff";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CanvasDocument {
    pub version: u32,
    pub viewport: CanvasViewport,
    pub elements: Vec<CanvasElement>,
    pub grid_enabled: bool,
    pub background_color: Option<String>,
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
            grid_enabled: true,
            background_color: None,
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
    }

    pub fn remove_element(&mut self, id: &str) -> Option<CanvasElement> {
        if let Some(pos) = self.elements.iter().position(|e| e.id == id) {
            Some(self.elements.remove(pos))
        } else {
            None
        }
    }

    pub fn bounding_box(&self) -> Option<(f32, f32, f32, f32)> {
        if self.elements.is_empty() {
            return None;
        }

        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;

        for elem in &self.elements {
            let (ex1, ey1, ex2, ey2) = match &elem.kind {
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
                _ => (elem.x, elem.y, elem.x + elem.width, elem.y + elem.height),
            };

            min_x = min_x.min(ex1);
            min_y = min_y.min(ey1);
            max_x = max_x.max(ex2);
            max_y = max_y.max(ey2);
        }

        Some((min_x, min_y, max_x, max_y))
    }

    pub fn find_element_at(&self, x: f32, y: f32) -> Option<&CanvasElement> {
        self.elements.iter().rev().find(|e| e.contains_point(x, y))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CanvasElement {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation: f32,
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
            rotation: 0.0,
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
            rotation: 0.0,
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
            rotation: 0.0,
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
            rotation: 0.0,
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
        let width = (content_str.len() as f32 * font_size * 0.6).max(20.0);
        let height = font_size * 1.4;

        Self {
            id: id.into(),
            x,
            y,
            width,
            height,
            rotation: 0.0,
            style: ElementStyle {
                stroke_color: "#abb2bf".to_string(),
                stroke_width: 1.0,
                stroke_style: StrokeStyle::Solid,
                fill_color: None,
                opacity: 1.0,
            },
            kind: ElementKind::Text {
                content: content_str,
                font_size,
                font_family: "system-ui".to_string(),
            },
        }
    }

    pub fn contains_point(&self, px: f32, py: f32) -> bool {
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
                if len_sq < 0.001 {
                    let dx = px - start_x;
                    let dy = py - start_y;
                    return (dx * dx + dy * dy) <= 36.0;
                }
                let t = (((px - start_x) * vx + (py - start_y) * vy) / len_sq).clamp(0.0, 1.0);
                let qx = start_x + t * vx;
                let qy = start_y + t * vy;
                let dx = px - qx;
                let dy = py - qy;
                (dx * dx + dy * dy) <= 36.0
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
    pub stroke_color: String,
    pub stroke_width: f32,
    pub stroke_style: StrokeStyle,
    pub fill_color: Option<String>,
    pub opacity: f32,
}

impl Default for ElementStyle {
    fn default() -> Self {
        Self {
            stroke_color: DEFAULT_STROKE_COLOR.to_string(),
            stroke_width: 2.0,
            stroke_style: StrokeStyle::Solid,
            fill_color: Some("#21252b".to_string()),
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

        assert!(doc.find_element_at(50.0, 40.0).is_some());
        assert_eq!(doc.find_element_at(50.0, 40.0).unwrap().id, "r1");
        assert!(doc.find_element_at(500.0, 500.0).is_none());
    }
}
