use ropey::Rope;
use std::ffi::c_void;
use std::time::{Duration, Instant};
use tree_sitter::{Language, Parser, Query, QueryCursor, StreamingIterator, Tree};

const SAMPLE_COUNT: usize = 5;
const LINE_COUNTS: [usize; 4] = [2_000, 20_000, 50_000, 100_000];
const LANGUAGES: [&str; 3] = ["rust", "dart", "typescript"];
const VIEWPORT_LINES: usize = 80;

#[derive(Default)]
struct StageSamples {
    rope_clone: Vec<Duration>,
    parser_setup: Vec<Duration>,
    full_parse: Vec<Duration>,
    query_compile: Vec<Duration>,
    viewport_query: Vec<Duration>,
    rss_delta_bytes: Vec<i64>,
}

#[cfg(windows)]
#[repr(C)]
struct ProcessMemoryCounters {
    cb: u32,
    page_fault_count: u32,
    peak_working_set_size: usize,
    working_set_size: usize,
    quota_peak_paged_pool_usage: usize,
    quota_paged_pool_usage: usize,
    quota_peak_non_paged_pool_usage: usize,
    quota_non_paged_pool_usage: usize,
    pagefile_usage: usize,
    peak_pagefile_usage: usize,
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcess() -> *mut c_void;
}

#[cfg(windows)]
#[link(name = "psapi")]
unsafe extern "system" {
    fn GetProcessMemoryInfo(
        process: *mut c_void,
        counters: *mut ProcessMemoryCounters,
        size: u32,
    ) -> i32;
}

#[cfg(windows)]
fn current_rss_bytes() -> Option<usize> {
    let mut counters = ProcessMemoryCounters {
        cb: std::mem::size_of::<ProcessMemoryCounters>() as u32,
        page_fault_count: 0,
        peak_working_set_size: 0,
        working_set_size: 0,
        quota_peak_paged_pool_usage: 0,
        quota_paged_pool_usage: 0,
        quota_peak_non_paged_pool_usage: 0,
        quota_non_paged_pool_usage: 0,
        pagefile_usage: 0,
        peak_pagefile_usage: 0,
    };
    let ok = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters,
            std::mem::size_of::<ProcessMemoryCounters>() as u32,
        )
    };
    (ok != 0).then_some(counters.working_set_size)
}

#[cfg(not(windows))]
fn current_rss_bytes() -> Option<usize> {
    None
}

fn micros(values: &[Duration]) -> Vec<u128> {
    values.iter().map(Duration::as_micros).collect()
}

fn percentile(values: &[Duration], fraction: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut values: Vec<f64> = values
        .iter()
        .map(|value| value.as_secs_f64() * 1_000.0)
        .collect();
    values.sort_by(f64::total_cmp);
    let index = (((values.len() - 1) as f64) * fraction).round() as usize;
    values[index]
}

fn mad_ms(values: &[Duration]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let median = percentile(values, 0.5);
    let mut deviations: Vec<f64> = values
        .iter()
        .map(|value| ((value.as_secs_f64() * 1_000.0) - median).abs())
        .collect();
    deviations.sort_by(f64::total_cmp);
    deviations[((deviations.len() - 1) as f64 * 0.5).round() as usize]
}

fn stats_json(values: &[Duration]) -> String {
    format!(
        "{{\"median_ms\":{:.3},\"p95_ms\":{:.3},\"mad_ms\":{:.3},\"samples_us\":{:?}}}",
        percentile(values, 0.5),
        percentile(values, 0.95),
        mad_ms(values),
        micros(values),
    )
}

fn signed_median(values: &[i64]) -> i64 {
    if values.is_empty() {
        return 0;
    }
    let mut values = values.to_vec();
    values.sort_unstable();
    values[(values.len() - 1) / 2]
}

fn language_profile(language_id: &str) -> (Language, String) {
    match language_id {
        "dart" => (
            tree_sitter_dart::LANGUAGE.into(),
            tree_sitter_dart::HIGHLIGHTS_QUERY.to_string(),
        ),
        "rust" => (
            tree_sitter_rust::LANGUAGE.into(),
            tree_sitter_rust::HIGHLIGHTS_QUERY.to_string(),
        ),
        "typescript" => (
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            format!(
                "{}\n{}",
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                tree_sitter_typescript::HIGHLIGHTS_QUERY
            ),
        ),
        other => panic!("unsupported C6P language {other}"),
    }
}

fn fixture(language_id: &str, lines: usize) -> String {
    let mut out = String::with_capacity(lines * 72);
    for index in 0..lines {
        match language_id {
            "rust" => {
                out.push_str(&format!(
                    "fn item_{index}() {{ let value_{index}: usize = {index}; println!(\"{{}}\", value_{index}); }}\n"
                ));
            }
            "dart" => {
                out.push_str(&format!(
                    "final value_{index} = 'needle alpha beta gamma {index}'; // benchmark\n"
                ));
            }
            "typescript" => {
                out.push_str(&format!(
                    "const value_{index}: string = `needle alpha beta gamma {index}`; // benchmark\n"
                ));
            }
            _ => unreachable!(),
        }
    }
    out
}

fn parse_rope(parser: &mut Parser, rope: &Rope) -> Option<Tree> {
    parser.parse_with_options(
        &mut |byte_offset, _position| {
            if byte_offset >= rope.len_bytes() {
                return &[][..];
            }
            let (chunk, chunk_byte_idx, _, _) = rope.chunk_at_byte(byte_offset);
            &chunk.as_bytes()[byte_offset - chunk_byte_idx..]
        },
        None,
        None,
    )
}

fn first_viewport_query(query: &Query, tree: &Tree, rope: &Rope) -> usize {
    let end_line = VIEWPORT_LINES.min(rope.len_lines()).max(1) - 1;
    let end_char = if end_line + 1 < rope.len_lines() {
        rope.line_to_char(end_line + 1)
    } else {
        rope.len_chars()
    };
    let end_byte = rope.char_to_byte(end_char);
    let mut cursor = QueryCursor::new();
    cursor.set_byte_range(0..end_byte);
    let mut captures = cursor.captures(query, tree.root_node(), |node: tree_sitter::Node<'_>| {
        let range = node.byte_range();
        let text = if range.start <= range.end && range.end <= rope.len_bytes() {
            rope.byte_slice(range).to_string()
        } else {
            String::new()
        };
        std::iter::once(text)
    });
    let mut count = 0usize;
    while captures.next().is_some() {
        count += 1;
    }
    count
}

fn main() {
    println!(
        "C6P_RUST_PROFILE_META {{\"sample_count\":{SAMPLE_COUNT},\"line_counts\":{:?},\"languages\":{:?},\"viewport_lines\":{VIEWPORT_LINES}}}",
        LINE_COUNTS, LANGUAGES
    );

    for language_id in LANGUAGES {
        for lines in LINE_COUNTS {
            let text = fixture(language_id, lines);
            let source_rope = Rope::from_str(&text);
            let bytes = text.len();
            let chars = text.chars().count();
            let mut samples = StageSamples::default();
            let mut viewport_capture_counts = Vec::with_capacity(SAMPLE_COUNT);

            // One untimed warm-up keeps one-time grammar/library setup out of the five samples.
            {
                let (language, highlight_query) = language_profile(language_id);
                let rope = source_rope.clone();
                let mut parser = Parser::new();
                parser.set_language(&language).expect("set language");
                let tree = parse_rope(&mut parser, &rope).expect("warm-up parse");
                let query = Query::new(&language, &highlight_query).expect("warm-up query compile");
                let _ = first_viewport_query(&query, &tree, &rope);
            }

            for _ in 0..SAMPLE_COUNT {
                let rss_before = current_rss_bytes();
                let started = Instant::now();
                let rope = source_rope.clone();
                samples.rope_clone.push(started.elapsed());

                let (language, highlight_query) = language_profile(language_id);
                let started = Instant::now();
                let mut parser = Parser::new();
                parser.set_language(&language).expect("set language");
                samples.parser_setup.push(started.elapsed());

                let started = Instant::now();
                let tree = parse_rope(&mut parser, &rope).expect("full parse");
                samples.full_parse.push(started.elapsed());

                let started = Instant::now();
                let query = Query::new(&language, &highlight_query).expect("compile query");
                samples.query_compile.push(started.elapsed());

                let started = Instant::now();
                let capture_count = first_viewport_query(&query, &tree, &rope);
                samples.viewport_query.push(started.elapsed());
                viewport_capture_counts.push(capture_count);
                if let (Some(before), Some(after)) = (rss_before, current_rss_bytes()) {
                    samples.rss_delta_bytes.push(after as i64 - before as i64);
                }
            }

            println!(
                "C6P_RUST_PROFILE {{\"language\":\"{language_id}\",\"lines\":{lines},\"bytes\":{bytes},\"chars\":{chars},\"rope_clone\":{},\"parser_setup\":{},\"full_parse\":{},\"query_compile\":{},\"first_viewport_query\":{},\"viewport_capture_counts\":{:?},\"rss_delta_bytes_median\":{},\"rss_delta_bytes_samples\":{:?}}}",
                stats_json(&samples.rope_clone),
                stats_json(&samples.parser_setup),
                stats_json(&samples.full_parse),
                stats_json(&samples.query_compile),
                stats_json(&samples.viewport_query),
                viewport_capture_counts,
                signed_median(&samples.rss_delta_bytes),
                samples.rss_delta_bytes,
            );
        }
    }

    profile_rapid_supersession();
}

fn profile_rapid_supersession() {
    const LINES: usize = 20_000;
    const GENERATIONS: usize = 3;
    let text = fixture("rust", LINES);
    let source_rope = Rope::from_str(&text);
    let mut wall_samples = Vec::with_capacity(SAMPLE_COUNT);
    let mut total_parse_work_samples = Vec::with_capacity(SAMPLE_COUNT);
    let mut useful_parse_samples = Vec::with_capacity(SAMPLE_COUNT);
    let mut wasted_parse_work_samples = Vec::with_capacity(SAMPLE_COUNT);
    let mut rss_delta_samples = Vec::with_capacity(SAMPLE_COUNT);

    for _ in 0..SAMPLE_COUNT {
        let rss_before = current_rss_bytes();
        let wall_started = Instant::now();
        let mut handles = Vec::with_capacity(GENERATIONS);
        for generation in 0..GENERATIONS {
            let rope = source_rope.clone();
            handles.push(std::thread::spawn(move || {
                let (language, highlight_query) = language_profile("rust");
                let started = Instant::now();
                let mut parser = Parser::new();
                parser.set_language(&language).expect("set language");
                let tree = parse_rope(&mut parser, &rope).expect("rapid parse");
                let query = Query::new(&language, &highlight_query).expect("rapid query compile");
                let _ = first_viewport_query(&query, &tree, &rope);
                (generation, started.elapsed())
            }));
        }

        let mut generation_times = vec![Duration::ZERO; GENERATIONS];
        for handle in handles {
            let (generation, elapsed) = handle.join().expect("rapid parser thread");
            generation_times[generation] = elapsed;
        }
        wall_samples.push(wall_started.elapsed());
        let useful = generation_times[GENERATIONS - 1];
        let total_work: Duration = generation_times.iter().copied().sum();
        useful_parse_samples.push(useful);
        total_parse_work_samples.push(total_work);
        wasted_parse_work_samples.push(total_work.saturating_sub(useful));
        if let (Some(before), Some(after)) = (rss_before, current_rss_bytes()) {
            rss_delta_samples.push(after as i64 - before as i64);
        }
    }

    println!(
        "C6P_RAPID_SUPERSESSION {{\"language\":\"rust\",\"lines\":{LINES},\"generations\":{GENERATIONS},\"only_last_generation_useful\":true,\"wall\":{},\"total_parse_work\":{},\"useful_parse\":{},\"wasted_parse_work\":{},\"theoretical_wasted_generation_fraction\":{:.3},\"rss_delta_bytes_median\":{},\"rss_delta_bytes_samples\":{:?}}}",
        stats_json(&wall_samples),
        stats_json(&total_parse_work_samples),
        stats_json(&useful_parse_samples),
        stats_json(&wasted_parse_work_samples),
        (GENERATIONS - 1) as f64 / GENERATIONS as f64,
        signed_median(&rss_delta_samples),
        rss_delta_samples,
    );
}
