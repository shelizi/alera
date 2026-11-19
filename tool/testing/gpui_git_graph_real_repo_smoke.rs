//! Portable smoke test for the real Git Graph topology model.
//! Build with `rustc --edition=2021 tool/testing/gpui_git_graph_real_repo_smoke.rs -o target/graph_smoke.exe`.
//! Feed `git log --topo-order -512 --format="%H %P"` to stdin.
//! A topology fallback is treated as a failed smoke, not as invented graph edges.

#[path = "../../desktop_gpui/src/git_graph_layout.rs"]
mod graph;

use std::io::{self, BufRead};

fn main() {
    let commits: Vec<graph::GraphCommit> = io::stdin()
        .lock()
        .lines()
        .map(|result| result.expect("failed to read Git history"))
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let mut columns = line.split_whitespace();
            graph::GraphCommit {
                oid: columns.next().expect("commit must have SHA").to_owned(),
                parents: columns.map(str::to_owned).collect(),
            }
        })
        .collect();
    if commits.is_empty() {
        eprintln!("FAIL: no commits provided");
        std::process::exit(2);
    }
    match graph::layout_git_graph(&commits) {
        Ok(layout) => {
            let merges = layout
                .rows
                .iter()
                .filter(|row| {
                    row.edges
                        .iter()
                        .filter(|edge| matches!(edge.kind, graph::GraphEdgeKind::Parent(_)))
                        .count()
                        > 1
                })
                .count();
            let edges: usize = layout.rows.iter().map(|row| row.edges.len()).sum();
            println!(
                "PASS rows={} max_lanes={} merges={} edges={}",
                layout.rows.len(),
                layout.max_lanes,
                merges,
                edges
            );
        }
        Err(error) => {
            eprintln!("FALLBACK rows={} reason={:?}", commits.len(), error);
            std::process::exit(2);
        }
    }
}
