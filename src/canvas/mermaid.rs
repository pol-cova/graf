//! Mermaid diagram parser and automatic vector canvas layout generator.

use std::collections::{HashMap, VecDeque};

use crate::canvas::scene::{CanvasDocument, CanvasElement, ElementStyle};

/// Flowchart orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutDirection {
    TopToBottom,
    LeftToRight,
}

/// Parsed Mermaid node definition.
#[derive(Debug, Clone, PartialEq)]
struct ParsedNode {
    id: String,
    label: String,
    shape: NodeShape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeShape {
    Rectangle,
    Rounded,
    Circle,
    Diamond,
}

/// Parsed Mermaid edge connecting two nodes.
#[derive(Debug, Clone, PartialEq)]
struct ParsedEdge {
    from: String,
    to: String,
    label: Option<String>,
    is_arrow: bool,
}

/// Parses Mermaid flowchart text and converts it into a native [`CanvasDocument`].
pub fn parse_mermaid_to_canvas(mermaid_text: &str) -> Result<CanvasDocument, String> {
    let mut direction = LayoutDirection::TopToBottom;
    let mut nodes: HashMap<String, ParsedNode> = HashMap::new();
    let mut node_order: Vec<String> = Vec::new();
    let mut edges: Vec<ParsedEdge> = Vec::new();

    for line in mermaid_text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("%%") {
            continue;
        }

        // Header detection (e.g. "graph TD", "graph LR", "flowchart TD")
        if trimmed.starts_with("graph") || trimmed.starts_with("flowchart") {
            let lower = trimmed.to_lowercase();
            if lower.contains("lr") || lower.contains("rl") {
                direction = LayoutDirection::LeftToRight;
            } else {
                direction = LayoutDirection::TopToBottom;
            }
            continue;
        }

        // Edge detection (e.g. "A --> B", "A -->|label| B", "A -- label --> B", "A --- B")
        if let Some(edge) = parse_edge_line(trimmed, &mut nodes, &mut node_order) {
            edges.push(edge);
            continue;
        }

        // Standalone node definition (e.g. "A[Start Box]")
        if let Some(node) = parse_single_node(trimmed) {
            if !nodes.contains_key(&node.id) {
                node_order.push(node.id.clone());
            }
            nodes.insert(node.id.clone(), node);
        }
    }

    if nodes.is_empty() {
        return Err("No valid Mermaid nodes found in input".to_string());
    }

    Ok(layout_graph(nodes, node_order, edges, direction))
}

fn parse_single_node(s: &str) -> Option<ParsedNode> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(open_idx) = trimmed.find(['[', '(', '{']) {
        let id = trimmed[..open_idx].trim().to_string();
        let rest = &trimmed[open_idx..];

        if rest.starts_with("((") && rest.ends_with("))") {
            let label = rest[2..rest.len() - 2].trim().to_string();
            return Some(ParsedNode {
                id,
                label,
                shape: NodeShape::Circle,
            });
        }
        if rest.starts_with('{') && rest.ends_with('}') {
            let label = rest[1..rest.len() - 1].trim().to_string();
            return Some(ParsedNode {
                id,
                label,
                shape: NodeShape::Diamond,
            });
        }
        if rest.starts_with('(') && rest.ends_with(')') {
            let label = rest[1..rest.len() - 1].trim().to_string();
            return Some(ParsedNode {
                id,
                label,
                shape: NodeShape::Rounded,
            });
        }
        if rest.starts_with('[') && rest.ends_with(']') {
            let label = rest[1..rest.len() - 1].trim().to_string();
            return Some(ParsedNode {
                id,
                label,
                shape: NodeShape::Rectangle,
            });
        }
    }

    // Default node where ID is also the label
    let id = trimmed
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect::<String>();
    if !id.is_empty() {
        Some(ParsedNode {
            id: id.clone(),
            label: id,
            shape: NodeShape::Rectangle,
        })
    } else {
        None
    }
}

fn parse_edge_line(
    line: &str,
    nodes: &mut HashMap<String, ParsedNode>,
    node_order: &mut Vec<String>,
) -> Option<ParsedEdge> {
    let arrow_tokens = ["-->|", "-->", "--", "-.->", "==>"];
    for token in arrow_tokens {
        if let Some(idx) = line.find(token) {
            let left_part = line[..idx].trim();
            let right_part = &line[idx + token.len()..];

            let left_node = parse_single_node(left_part)?;
            if !nodes.contains_key(&left_node.id) {
                node_order.push(left_node.id.clone());
            }
            nodes.insert(left_node.id.clone(), left_node.clone());

            let (label, right_target) = if token == "-->|" {
                if let Some(pipe_end) = right_part.find('|') {
                    let lbl = right_part[..pipe_end].trim().to_string();
                    let target = right_part[pipe_end + 1..].trim();
                    (Some(lbl), target)
                } else {
                    (None, right_part.trim())
                }
            } else if token == "--" {
                if let Some(arr_idx) = right_part.find("-->") {
                    let lbl = right_part[..arr_idx].trim().to_string();
                    let target = right_part[arr_idx + 3..].trim();
                    (Some(lbl), target)
                } else {
                    (None, right_part.trim())
                }
            } else {
                (None, right_part.trim())
            };

            let right_node = parse_single_node(right_target)?;
            if !nodes.contains_key(&right_node.id) {
                node_order.push(right_node.id.clone());
            }
            nodes.insert(right_node.id.clone(), right_node.clone());

            return Some(ParsedEdge {
                from: left_node.id,
                to: right_node.id,
                label,
                is_arrow: token.contains('>') || token == "-->",
            });
        }
    }
    None
}

/// Assigns topological coordinates and constructs vector elements.
fn layout_graph(
    nodes: HashMap<String, ParsedNode>,
    node_order: Vec<String>,
    edges: Vec<ParsedEdge>,
    direction: LayoutDirection,
) -> CanvasDocument {
    let mut doc = CanvasDocument::new();

    // 1. Calculate node layers using BFS
    let mut in_degrees: HashMap<String, usize> = HashMap::new();
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    for id in &node_order {
        in_degrees.insert(id.clone(), 0);
        adj.insert(id.clone(), Vec::new());
    }
    for e in &edges {
        if let Some(entry) = adj.get_mut(&e.from) {
            entry.push(e.to.clone());
        }
        if let Some(deg) = in_degrees.get_mut(&e.to) {
            *deg += 1;
        }
    }

    let mut layers: HashMap<String, usize> = HashMap::new();
    let mut queue = VecDeque::new();
    for (id, &deg) in &in_degrees {
        if deg == 0 {
            queue.push_back((id.clone(), 0));
            layers.insert(id.clone(), 0);
        }
    }

    if queue.is_empty() && !node_order.is_empty() {
        queue.push_back((node_order[0].clone(), 0));
        layers.insert(node_order[0].clone(), 0);
    }

    while let Some((id, level)) = queue.pop_front() {
        if let Some(neighbors) = adj.get(&id) {
            for next in neighbors {
                let next_lvl = level + 1;
                let current_max = layers.get(next).copied().unwrap_or(0);
                if next_lvl > current_max {
                    layers.insert(next.clone(), next_lvl);
                    queue.push_back((next.clone(), next_lvl));
                }
            }
        }
    }

    for id in &node_order {
        layers.entry(id.clone()).or_insert(0);
    }

    // 2. Group nodes by layer
    let mut layer_groups: HashMap<usize, Vec<String>> = HashMap::new();
    for (id, &layer) in &layers {
        layer_groups.entry(layer).or_default().push(id.clone());
    }

    let node_w = 140.0;
    let node_h = 50.0;
    let gap_x = 80.0;
    let gap_y = 70.0;

    let mut node_positions: HashMap<String, (f32, f32)> = HashMap::new();

    let mut sorted_layers: Vec<usize> = layer_groups.keys().copied().collect();
    sorted_layers.sort();

    for layer in sorted_layers {
        let group = &layer_groups[&layer];
        let count = group.len();

        for (i, id) in group.iter().enumerate() {
            let (x, y) = match direction {
                LayoutDirection::TopToBottom => {
                    let total_layer_w =
                        count as f32 * node_w + (count.saturating_sub(1)) as f32 * gap_x;
                    let start_x = 400.0 - total_layer_w / 2.0;
                    let x = start_x + i as f32 * (node_w + gap_x);
                    let y = 100.0 + layer as f32 * (node_h + gap_y);
                    (x, y)
                }
                LayoutDirection::LeftToRight => {
                    let total_layer_h =
                        count as f32 * node_h + (count.saturating_sub(1)) as f32 * gap_y;
                    let start_y = 300.0 - total_layer_h / 2.0;
                    let x = 100.0 + layer as f32 * (node_w + gap_x);
                    let y = start_y + i as f32 * (node_h + gap_y);
                    (x, y)
                }
            };
            node_positions.insert(id.clone(), (x, y));
        }
    }

    // 3. Create node shapes and text elements
    let mut elem_counter = 1;
    for id in &node_order {
        if let Some(node) = nodes.get(id) {
            let (nx, ny) = node_positions[id];
            let radius = match node.shape {
                NodeShape::Rounded => 12.0,
                NodeShape::Rectangle => 4.0,
                _ => 6.0,
            };

            let mut shape = if node.shape == NodeShape::Circle {
                CanvasElement::new_ellipse(format!("node_{elem_counter}"), nx, ny, node_w, node_h)
            } else {
                CanvasElement::new_rectangle(
                    format!("node_{elem_counter}"),
                    nx,
                    ny,
                    node_w,
                    node_h,
                    radius,
                )
            };
            shape.style.fill_color = Some("#252b3b".to_string());
            shape.style.stroke_color = "#3b82f6".to_string();
            shape.style.stroke_width = 2.0;
            doc.add_element(shape);
            elem_counter += 1;

            let text_elem = CanvasElement::new_text(
                format!("text_{elem_counter}"),
                nx + 14.0,
                ny + 16.0,
                &node.label,
                14.0,
            );
            doc.add_element(text_elem);
            elem_counter += 1;
        }
    }

    // 4. Create connector edges and arrows
    for e in &edges {
        if let (Some(&(x1, y1)), Some(&(x2, y2))) =
            (node_positions.get(&e.from), node_positions.get(&e.to))
        {
            let (sx, sy, ex, ey) = match direction {
                LayoutDirection::TopToBottom => {
                    (x1 + node_w / 2.0, y1 + node_h, x2 + node_w / 2.0, y2)
                }
                LayoutDirection::LeftToRight => {
                    (x1 + node_w, y1 + node_h / 2.0, x2, y2 + node_h / 2.0)
                }
            };

            let mut edge_elem = if e.is_arrow {
                CanvasElement::new_arrow(format!("edge_{elem_counter}"), sx, sy, ex, ey)
            } else {
                CanvasElement::new_line(format!("edge_{elem_counter}"), sx, sy, ex, ey)
            };
            edge_elem.style = ElementStyle {
                stroke_color: "#60a5fa".to_string(),
                stroke_width: 2.0,
                ..Default::default()
            };
            doc.add_element(edge_elem);
            elem_counter += 1;

            if let Some(lbl) = &e.label {
                let mid_x = (sx + ex) / 2.0 + 4.0;
                let mid_y = (sy + ey) / 2.0 - 8.0;
                let lbl_elem =
                    CanvasElement::new_text(format!("lbl_{elem_counter}"), mid_x, mid_y, lbl, 12.0);
                doc.add_element(lbl_elem);
                elem_counter += 1;
            }
        }
    }

    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mermaid_top_down() {
        let input = r#"
graph TD
    A[Start Process] --> B[Execute Model]
    B --> C[Verify Accuracy]
"#;
        let doc = parse_mermaid_to_canvas(input).expect("Should parse valid mermaid graph");
        assert!(doc.elements.len() >= 5); // 3 shapes + 3 texts + 2 arrows
    }

    #[test]
    fn test_parse_mermaid_with_labels_and_shapes() {
        let input = r#"
flowchart LR
    A(Input Data) -->|train| B[Neural Network]
    B --> C{Evaluate}
"#;
        let doc = parse_mermaid_to_canvas(input).expect("Should parse flowchart LR");
        assert!(doc.elements.len() >= 6);
    }
}
