//! Run standalone with:
//! rustc --edition=2021 -O benchmark/git_intraline_alignment_benchmark.rs -o .git/git-intraline-bench.exe
//! .git/git-intraline-bench.exe
//! This includes the production Rust module directly; no Flutter/Dart build needed.
use std::hint::black_box;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitDiffLineKind {
    Addition,
    Deletion,
    Context,
    Header,
    Hunk,
}
pub struct GitDiffLine {
    pub kind: GitDiffLineKind,
    pub text: String,
}
#[allow(dead_code)]
mod intraline {
    include!("../rust/src/api/git_intraline.rs");
}

fn fixture(count: usize, width: usize) -> (Vec<String>, Vec<String>) {
    let mut old = Vec::with_capacity(count);
    let mut new = Vec::with_capacity(count);
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    for i in 0..count {
        let mut content = String::with_capacity(width + 32);
        for j in 0..width {
            content.push(ALPHABET[(i * 13 + j * 7 + j / 4) % ALPHABET.len()] as char);
        }
        old.push(format!("item_{i:03} {content} done"));
        let mut changed = content.clone().into_bytes();
        changed[width / 2] = b'Z';
        let changed = String::from_utf8(changed).expect("ASCII fixture");
        new.push(format!("item_{i:03} {changed} done"));
    }
    (old, new)
}

fn run(label: &str, count: usize, width: usize, iterations: usize) {
    let (old, new) = fixture(count, width);
    let old: Vec<&str> = old.iter().map(String::as_str).collect();
    let new: Vec<&str> = new.iter().map(String::as_str).collect();
    let mut samples = Vec::with_capacity(7);
    for _ in 0..7 {
        let began = Instant::now();
        for _ in 0..iterations {
            black_box(intraline::align_changed_lines(
                black_box(&old),
                black_box(&new),
            ));
        }
        samples.push(began.elapsed().as_secs_f64() * 1_000_000.0 / iterations as f64);
    }
    samples.sort_by(f64::total_cmp);
    println!("{label}: median_us={:.2} min_us={:.2} max_us={:.2} ({count}x{count}, chars={width}, iterations={iterations})",
        samples[3], samples[0], samples[6]);
}

fn main() {
    run("short", 10, 28, 180);
    run("medium", 36, 48, 28);
    run("long", 64, 95, 28);
}
