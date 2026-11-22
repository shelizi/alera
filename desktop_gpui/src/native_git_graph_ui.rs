//! Native Git Graph lane painting. The immutable layout comes from the
//! headless graph module; this GPUI canvas never reads or modifies a repo.
use alera_gpui_desktop::git_graph_layout::{GraphEdgeKind, GraphLaneRow, MAX_GRAPH_LANES};
use gpui::{canvas, point, prelude::*, px, rgb, IntoElement, PathBuilder, Pixels, Point, Window};

const PITCH: f32 = 16.0;
const COMPACT_PITCH: f32 = 12.0;
// Match Classic GitHistoryGraphPainter.laneHeight and its 28px commit rows.
const HEIGHT: f32 = 28.0;
const RAIL_BLEED: f32 = 1.0;
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

// The sidebar has variable-height rows when commit text and labels wrap.
// Rails must cover the measured canvas rather than a hard-coded 28px.
#[derive(Debug, Clone, Copy)]
struct RailY {
    top: f32,
    middle: f32,
    bend: f32,
    bottom: f32,
}

fn rail_y(actual_height: f32) -> RailY {
    let height = actual_height.max(HEIGHT);
    let middle = height / 2.0;
    RailY {
        top: -RAIL_BLEED,
        middle,
        bend: (middle + height) / 2.0,
        bottom: height + RAIL_BLEED,
    }
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

    #[test]
    fn rail_geometry_covers_wrapped_sidebar_rows_and_overlaps_neighbors() {
        for height in [28.0, 36.0, 44.0, 60.0] {
            let y = rail_y(height);
            assert_eq!(y.middle, height / 2.0);
            assert_eq!(y.bend, height * 0.75);
            assert!(y.top < 0.0 && y.bottom > height);
            assert!(y.bottom > height + rail_y(28.0).top);
        }
    }
}

/// Fixed 28px vector rail for center-pane commit rows. Tighten lane spacing
/// in narrow windows without dropping edges; geometry uses logical pixels.
pub fn graph_lane_with_density(
    layout: Option<GraphLaneRow>,
    max_lanes: usize,
    is_merge: bool,
    is_head: bool,
    compact: bool,
) -> impl IntoElement {
    graph_lane_impl(layout, max_lanes, is_merge, is_head, compact, false)
}

/// For variable-height Git Tree rows in the Source Control sidebar.
/// The containing flex row must use `items_stretch()`.
pub fn graph_lane_stretched(
    layout: Option<GraphLaneRow>,
    max_lanes: usize,
    is_merge: bool,
    is_head: bool,
) -> impl IntoElement {
    graph_lane_impl(layout, max_lanes, is_merge, is_head, false, true)
}

fn graph_lane_impl(
    layout: Option<GraphLaneRow>,
    max_lanes: usize,
    is_merge: bool,
    is_head: bool,
    compact: bool,
    stretch_height: bool,
) -> impl IntoElement {
    let pitch = if compact { COMPACT_PITCH } else { PITCH };
    let columns = if layout.is_some() {
        max_lanes.clamp(1, MAX_GRAPH_LANES)
    } else {
        1
    };
    let rail = canvas(
        |bounds, _, _| bounds,
        move |bounds, _, window, _| {
            let y = rail_y(f32::from(bounds.size.height));
            let at = |lane: usize, y: f32| {
                point(
                    bounds.left() + px(pitch * (lane as f32 + 1.0)),
                    bounds.top() + px(y),
                )
            };
            let (node_lane, node_color) = if let Some(row) = layout.as_ref() {
                // Through-lanes are drawn together with their outgoing
                // edges as a single path; separate half-strokes can crack.
                // Only the node lane needs an independent incoming stem.
                if row.has_incoming {
                    stroke(
                        window,
                        lane_color(row.node_color),
                        at(row.node_lane, y.top),
                        at(row.node_lane, y.middle),
                        None,
                        None,
                    );
                }
                for edge in &row.edges {
                    let color = row
                        .lane_colors_after
                        .get(edge.to_lane)
                        .map(|id| lane_color(*id))
                        .unwrap_or(lane_color(row.node_color));
                    let through = matches!(edge.kind, GraphEdgeKind::Through);
                    let start = at(edge.from_lane, if through { y.top } else { y.middle });
                    let end = at(edge.to_lane, y.bottom);
                    if edge.from_lane == edge.to_lane {
                        stroke(window, color, start, end, None, None);
                    } else {
                        // Keep a shifting through-lane continuous before
                        // curving towards its column in the next row.
                        let mut builder = PathBuilder::stroke(px(1.5));
                        builder.move_to(start);
                        if through {
                            builder.line_to(at(edge.from_lane, y.middle));
                        }
                        builder.cubic_bezier_to(
                            end,
                            at(edge.from_lane, y.bend),
                            at(edge.to_lane, y.bend),
                        );
                        if let Ok(path) = builder.build() {
                            window.paint_path(path, rgb(color));
                        }
                    }
                }
                (row.node_lane, lane_color(row.node_color))
            } else {
                (0, PALETTE[0])
            };
            let center = at(node_lane, y.middle);
            if is_head {
                circle(
                    window,
                    center,
                    if compact { 6.0 } else { 8.0 },
                    0xE0E0E0,
                    true,
                );
            }
            if is_merge {
                circle(
                    window,
                    center,
                    if compact { 4.0 } else { 4.8 },
                    node_color,
                    true,
                );
            } else {
                circle(
                    window,
                    center,
                    if compact { 3.6 } else { 4.2 },
                    node_color,
                    false,
                );
            }
        },
    )
    .w(px(graph_lane_width(columns, pitch)));
    if stretch_height {
        rail.min_h(px(HEIGHT)).into_any_element()
    } else {
        rail.h(px(HEIGHT)).into_any_element()
    }
}
