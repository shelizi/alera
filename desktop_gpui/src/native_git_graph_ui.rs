//! Native Git Graph lane painting. The immutable layout comes from the
//! headless graph module; this GPUI canvas never reads or modifies a repo.
use alera_gpui_desktop::git_graph_layout::{GraphLaneRow, MAX_GRAPH_LANES};
use gpui::{canvas, point, prelude::*, px, rgb, IntoElement, PathBuilder, Pixels, Point, Window};

const PITCH: f32 = 16.0;
const COMPACT_PITCH: f32 = 12.0;
// Match Classic GitHistoryGraphPainter.laneHeight and its 28px commit rows.
const HEIGHT: f32 = 28.0;
const MID_Y: f32 = HEIGHT / 2.0;
// Identical to Classic's AleraTokens GitHistoryGraphColorId palette.
pub const REF_LOCAL_COLOR: u32 = 0x22C55E;
pub const REF_REMOTE_COLOR: u32 = 0x60A5FA;
pub const REF_TAG_COLOR: u32 = 0xF59E0B;
const PALETTE: [u32; 8] = [
    REF_LOCAL_COLOR,
    REF_REMOTE_COLOR,
    REF_TAG_COLOR,
    0x82AAFF, // syntaxFunction (lane1)
    0xC792EA, // syntaxKeyword (lane2)
    0xFFCB6B, // syntaxLiteral (lane3)
    0x89DDFF, // syntaxOperator (lane4)
    0xA1A1A1, // foregroundMuted (lane5)
];

fn lane_color(color_id: usize) -> u32 {
    PALETTE[color_id % PALETTE.len()]
}

fn stroke(
    window: &mut Window,
    color: u32,
    start: Point<Pixels>,
    end: Point<Pixels>,
    control_a: Option<Point<Pixels>>,
    control_b: Option<Point<Pixels>>,
) {
    let mut builder = PathBuilder::stroke(px(1.5));
    builder.move_to(start);
    if let (Some(a), Some(b)) = (control_a, control_b) {
        builder.cubic_bezier_to(end, a, b);
    } else {
        builder.line_to(end);
    }
    if let Ok(path) = builder.build() {
        window.paint_path(path, rgb(color));
    }
}

// Four cubic segments make a smooth, DPI-independent circle without
// allocating images or importing a second rendering engine.
fn circle(window: &mut Window, center: Point<Pixels>, radius: f32, color: u32, outline: bool) {
    let mut builder = if outline {
        PathBuilder::stroke(px(1.5))
    } else {
        PathBuilder::fill()
    };
    let x = center.x;
    let y = center.y;
    let r = px(radius);
    let k = px(radius * 0.5522848);
    builder.move_to(point(x + r, y));
    builder.cubic_bezier_to(point(x, y + r), point(x + r, y + k), point(x + k, y + r));
    builder.cubic_bezier_to(point(x - r, y), point(x - k, y + r), point(x - r, y + k));
    builder.cubic_bezier_to(point(x, y - r), point(x - r, y - k), point(x - k, y - r));
    builder.cubic_bezier_to(point(x + r, y), point(x + k, y - r), point(x + r, y - k));
    builder.close();
    if let Ok(path) = builder.build() {
        window.paint_path(path, rgb(color));
    }
}

fn graph_lane_width(columns: usize, pitch: f32) -> f32 {
    (columns.min(MAX_GRAPH_LANES) as f32 + 1.0) * pitch
}

#[cfg(test)]
mod compact_layout_tests {
    use super::*;

    #[test]
    fn compact_twelve_lanes_preserve_all_columns_in_logical_pixels() {
        assert_eq!(graph_lane_width(MAX_GRAPH_LANES, PITCH), 208.0);
        assert_eq!(graph_lane_width(MAX_GRAPH_LANES, COMPACT_PITCH), 156.0);
        assert_eq!(graph_lane_width(1, COMPACT_PITCH), 24.0);
        assert_eq!(graph_lane_width(MAX_GRAPH_LANES + 10, COMPACT_PITCH), 156.0);
    }
}

/// Each row is a 28px high native vector rail, matching Classic's commit
/// row geometry. The parent div remains the accessible click target.
pub fn graph_lane(
    layout: Option<GraphLaneRow>,
    max_lanes: usize,
    is_merge: bool,
    is_head: bool,
) -> impl IntoElement {
    graph_lane_with_density(layout, max_lanes, is_merge, is_head, false)
}

/// Tighten lane spacing in split panes or narrow windows, but never drop
/// commit/parent edges. Geometry is in GPUI logical pixels at any DPI.
pub fn graph_lane_with_density(
    layout: Option<GraphLaneRow>,
    max_lanes: usize,
    is_merge: bool,
    is_head: bool,
    compact: bool,
) -> impl IntoElement {
    let pitch = if compact { COMPACT_PITCH } else { PITCH };
    let columns = if layout.is_some() {
        max_lanes.clamp(1, MAX_GRAPH_LANES)
    } else {
        1
    };
    canvas(
        |bounds, _, _| bounds,
        move |bounds, _, window, _| {
            let at = |lane: usize, y: f32| {
                point(
                    bounds.left() + px(pitch * (lane as f32 + 1.0)),
                    bounds.top() + px(y),
                )
            };
            let (node_lane, node_color) = if let Some(row) = layout.as_ref() {
                // Through-lanes stay colored even when inserting or removing a
                // branch moves them horizontally between adjacent rows.
                for (lane, color) in row.lane_colors_before.iter().enumerate() {
                    if lane != row.node_lane || row.has_incoming {
                        stroke(
                            window,
                            lane_color(*color),
                            at(lane, -0.5),
                            at(lane, MID_Y),
                            None,
                            None,
                        );
                    }
                }
                for edge in &row.edges {
                    let color = row
                        .lane_colors_after
                        .get(edge.to_lane)
                        .map(|id| lane_color(*id))
                        .unwrap_or(lane_color(row.node_color));
                    let start = at(edge.from_lane, MID_Y);
                    let end = at(edge.to_lane, HEIGHT + 0.5);
                    if edge.from_lane == edge.to_lane {
                        stroke(window, color, start, end, None, None);
                    } else {
                        // Smooth convergence like Classic's CustomPainter.
                        stroke(
                            window,
                            color,
                            start,
                            end,
                            Some(at(edge.from_lane, HEIGHT - 7.0)),
                            Some(at(edge.to_lane, MID_Y + 7.0)),
                        );
                    }
                }
                (row.node_lane, lane_color(row.node_color))
            } else {
                (0, PALETTE[0])
            };
            let center = at(node_lane, MID_Y);
            if is_head {
                circle(window, center, if compact { 6.0 } else { 8.0 }, 0xE0E0E0, true);
            }
            if is_merge {
                circle(window, center, if compact { 4.0 } else { 4.8 }, node_color, true);
            } else {
                circle(window, center, if compact { 3.6 } else { 4.2 }, node_color, false);
            }
        },
    )
    .w(px(graph_lane_width(columns, pitch)))
    .h(px(HEIGHT))
}
