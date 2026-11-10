//! Bounded, UI-agnostic Git Graph lane layout.
//!
//! The input must be a topologically ordered list of exact Git commit IDs
//! (children before parents). Renderers use the generated light box-drawing
//! rails and true parent edges instead of guessing from the parent count.
//! An unsupported history returns a clear error; no fake connections.

use std::collections::HashSet;

pub const MAX_GRAPH_COMMITS: usize = 96;
pub const MAX_GRAPH_LANES: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphCommit {
    pub oid: String,
    pub parents: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphEdgeKind {
    Through,
    Parent(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphEdge {
    pub from_lane: usize,
    pub to_lane: usize,
    pub kind: GraphEdgeKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphLaneRow {
    pub oid: String,
    pub node_lane: usize,
    pub edges: Vec<GraphEdge>,
    /// A Unicode rail with one node (● or ◆) and any pending branch lanes.
    pub node_line: String,
    /// Exact topology between this node and the next commit row.
    pub connector_line: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitGraphLayout {
    pub rows: Vec<GraphLaneRow>,
    pub max_lanes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphLayoutError {
    TooManyCommits,
    TooManyLanes,
    InvalidCommit,
    DuplicateCommit,
    DuplicateParent,
    WrongCommitOrder,
}

impl GraphLayoutError {
    pub fn message(self) -> &'static str {
        match self {
            Self::TooManyCommits => "Graph capped at 96 commits",
            Self::TooManyLanes => "More than 12 simultaneous branches",
            Self::InvalidCommit => "Commit or parent ID is empty",
            Self::DuplicateCommit => "Duplicate commit in history",
            Self::DuplicateParent => "Repeated commit parent",
            Self::WrongCommitOrder => "History is not topological (child before parent)",
        }
    }
}

#[derive(Debug)]
struct PendingRow {
    oid: String,
    node_lane: usize,
    before_lanes: usize,
    after_lanes: usize,
    is_merge: bool,
    edges: Vec<GraphEdge>,
}

/// No repository I/O, no mutation, bounded CPU and memory.
pub fn layout_git_graph(commits: &[GraphCommit]) -> Result<GitGraphLayout, GraphLayoutError> {
    if commits.len() > MAX_GRAPH_COMMITS {
        return Err(GraphLayoutError::TooManyCommits);
    }

    let mut seen = HashSet::<String>::new();
    let mut live = Vec::<String>::new();
    let mut pending = Vec::with_capacity(commits.len());
    let mut max_lanes = 1;

    for commit in commits {
        if commit.oid.is_empty() || commit.parents.iter().any(String::is_empty) {
            return Err(GraphLayoutError::InvalidCommit);
        }
        if !seen.insert(commit.oid.clone()) {
            return Err(GraphLayoutError::DuplicateCommit);
        }
        let mut unique = HashSet::<&str>::new();
        for parent in &commit.parents {
            if parent == &commit.oid || seen.contains(parent) {
                return Err(GraphLayoutError::WrongCommitOrder);
            }
            if !unique.insert(parent) {
                return Err(GraphLayoutError::DuplicateParent);
            }
        }

        let node_lane = match live.iter().position(|oid| oid == &commit.oid) {
            Some(existing) => existing,
            None => {
                if live.len() >= MAX_GRAPH_LANES {
                    return Err(GraphLayoutError::TooManyLanes);
                }
                live.push(commit.oid.clone());
                live.len() - 1
            }
        };
        let before = live.clone();
        live.remove(node_lane);

        // Reuse a pending parent lane rather than drawing a second disconnected
        // parent node. Insert first-parent at the removed node lane when free.
        let mut insert_at = node_lane.min(live.len());
        for parent in &commit.parents {
            if !live.iter().any(|tracked| tracked == parent) {
                if live.len() >= MAX_GRAPH_LANES {
                    return Err(GraphLayoutError::TooManyLanes);
                }
                live.insert(insert_at, parent.clone());
                insert_at += 1;
            }
        }

        let mut edges = Vec::new();
        for (from_lane, id) in before.iter().enumerate() {
            if from_lane == node_lane {
                continue;
            }
            let to_lane = live
                .iter()
                .position(|entry| entry == id)
                .expect("pending non-node lane is always retained");
            edges.push(GraphEdge {
                from_lane,
                to_lane,
                kind: GraphEdgeKind::Through,
            });
        }
        for (parent_index, id) in commit.parents.iter().enumerate() {
            let to_lane = live
                .iter()
                .position(|entry| entry == id)
                .expect("every parent was inserted or already pending");
            edges.push(GraphEdge {
                from_lane: node_lane,
                to_lane,
                kind: GraphEdgeKind::Parent(parent_index),
            });
        }

        max_lanes = max_lanes.max(before.len()).max(live.len());
        pending.push(PendingRow {
            oid: commit.oid.clone(),
            node_lane,
            before_lanes: before.len(),
            after_lanes: live.len(),
            is_merge: commit.parents.len() > 1,
            edges,
        });
    }
    let rows = pending
        .into_iter()
        .map(|row| {
            let mut symbols = vec![' '; max_lanes * 2 - 1];
            for lane in 0..row.before_lanes {
                symbols[lane * 2] = if lane == row.node_lane {
                    if row.is_merge {
                        '◆'
                    } else {
                        '●'
                    }
                } else {
                    '│'
                };
            }
            let node_line: String = symbols.into_iter().collect();
            let connector_line = render_connectors(&row.edges, max_lanes);
            debug_assert!(row.after_lanes <= max_lanes);
            GraphLaneRow {
                oid: row.oid,
                node_lane: row.node_lane,
                edges: row.edges,
                node_line,
                connector_line,
            }
        })
        .collect();
    Ok(GitGraphLayout { rows, max_lanes })
}

// Box drawing connectivity masks: U, D, L, R. Merging masks preserves
// branches, bends, and crossings rather than dropping extra parent edges.
const UP: u8 = 1;
const DOWN: u8 = 2;
const LEFT: u8 = 4;
const RIGHT: u8 = 8;

fn rail(mask: u8) -> char {
    match mask {
        0 => ' ',
        UP => '╵',
        DOWN => '╷',
        LEFT => '╴',
        RIGHT => '╶',
        3 => '│',
        12 => '─',
        9 => '└',
        5 => '┘',
        10 => '┌',
        6 => '┐',
        11 => '├',
        7 => '┤',
        13 => '┴',
        14 => '┬',
        15 => '┼',
        _ => ' ',
    }
}

fn render_connectors(edges: &[GraphEdge], max_lanes: usize) -> String {
    let mut masks = vec![0u8; max_lanes * 2 - 1];
    for edge in edges {
        let from = edge.from_lane * 2;
        let to = edge.to_lane * 2;
        if from == to {
            masks[from] |= UP | DOWN;
        } else if from < to {
            masks[from] |= UP | RIGHT;
            for mask in &mut masks[from + 1..to] {
                *mask |= LEFT | RIGHT;
            }
            masks[to] |= LEFT | DOWN;
        } else {
            masks[from] |= UP | LEFT;
            for mask in &mut masks[to + 1..from] {
                *mask |= LEFT | RIGHT;
            }
            masks[to] |= RIGHT | DOWN;
        }
    }
    masks.into_iter().map(rail).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(oid: &str, parents: &[&str]) -> GraphCommit {
        GraphCommit {
            oid: oid.to_owned(),
            parents: parents.iter().map(|parent| (*parent).to_owned()).collect(),
        }
    }

    #[test]
    fn linear_history_uses_one_lane_and_correct_parent_links() {
        let graph = layout_git_graph(&[
            c("new", &["middle"]),
            c("middle", &["root"]),
            c("root", &[]),
        ])
        .unwrap();
        assert_eq!(graph.max_lanes, 1);
        assert_eq!(
            graph
                .rows
                .iter()
                .map(|row| row.node_line.as_str())
                .collect::<Vec<_>>(),
            vec!["●", "●", "●"]
        );
        assert_eq!(graph.rows[0].connector_line, "│");
        assert!(graph.rows[2].edges.is_empty());
    }

    #[test]
    fn merge_exposes_two_parents_and_real_convergence() {
        let graph = layout_git_graph(&[
            c("merge", &["left", "right"]),
            c("left", &["shared"]),
            c("right", &["shared"]),
            c("shared", &["root"]),
            c("root", &[]),
        ])
        .unwrap();
        assert_eq!(graph.max_lanes, 2);
        assert_eq!(graph.rows[0].node_line, "◆  ");
        assert_eq!(graph.rows[0].connector_line, "├─┐");
        assert_eq!(graph.rows[0].edges.len(), 2);
        assert_eq!(graph.rows[2].node_lane, 1);
        assert!(graph.rows[2].edges.contains(&GraphEdge {
            from_lane: 1,
            to_lane: 0,
            kind: GraphEdgeKind::Parent(0),
        }));
        assert_eq!(graph.rows[3].node_lane, 0);
        assert_eq!(graph.rows[4].edges.len(), 0);
    }

    #[test]
    fn unrelated_tips_never_receive_fake_edges() {
        let graph = layout_git_graph(&[c("a", &[]), c("b", &[])]).unwrap();
        assert!(graph.rows.iter().all(|row| row.edges.is_empty()));
    }

    #[test]
    fn malformed_inputs_fail_instead_of_drawing_wrong_lines() {
        assert_eq!(
            layout_git_graph(&[c("a", &["a"])]),
            Err(GraphLayoutError::WrongCommitOrder)
        );
        assert_eq!(
            layout_git_graph(&[c("a", &[]), c("a", &[])]),
            Err(GraphLayoutError::DuplicateCommit)
        );
        assert_eq!(
            layout_git_graph(&[c("a", &["b", "b"])]),
            Err(GraphLayoutError::DuplicateParent)
        );
        assert_eq!(
            layout_git_graph(&[c("b", &[]), c("a", &["b"])]),
            Err(GraphLayoutError::WrongCommitOrder)
        );
        assert_eq!(
            layout_git_graph(&[c("", &[])]),
            Err(GraphLayoutError::InvalidCommit)
        );
    }

    #[test]
    fn explicit_row_and_lane_budgets_are_enforced() {
        let commits = (0..=MAX_GRAPH_COMMITS)
            .map(|i| c(&i.to_string(), &[]))
            .collect::<Vec<_>>();
        assert_eq!(
            layout_git_graph(&commits),
            Err(GraphLayoutError::TooManyCommits)
        );
        let parents = (0..=MAX_GRAPH_LANES)
            .map(|i| format!("parent-{i}"))
            .collect();
        assert_eq!(
            layout_git_graph(&[GraphCommit {
                oid: "merge".into(),
                parents
            }]),
            Err(GraphLayoutError::TooManyLanes)
        );
    }
}
