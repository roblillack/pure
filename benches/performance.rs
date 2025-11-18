use std::time::{Duration, Instant};
use tdoc::{Document, InlineStyle, Paragraph, ParagraphType, Span};
use pure::render::RenderSentinels;

/// Performance benchmark suite for Pure editor operations
///
/// Run with: cargo test --release --bench performance -- --nocapture
///
/// This measures:
/// - Document rendering performance
/// - Cursor movement operations
/// - Character insertion/deletion
/// - Segment rebuilding
/// - Scrolling operations

const SMALL_DOC_PARAGRAPHS: usize = 10;
const MEDIUM_DOC_PARAGRAPHS: usize = 100;
const LARGE_DOC_PARAGRAPHS: usize = 1000;
const HUGE_DOC_PARAGRAPHS: usize = 10000;

const ITERATIONS: usize = 100;

const NO_SENTINELS: RenderSentinels = RenderSentinels {
    cursor: '\0',
    selection_start: '\0',
    selection_end: '\0',
};

/// Create a test document with the specified number of paragraphs
fn create_test_document(num_paragraphs: usize, avg_words_per_para: usize) -> Document {
    let mut doc = Document::new();

    let sample_words = vec![
        "Lorem", "ipsum", "dolor", "sit", "amet", "consectetur", "adipiscing", "elit",
        "sed", "do", "eiusmod", "tempor", "incididunt", "ut", "labore", "et", "dolore",
        "magna", "aliqua", "Ut", "enim", "ad", "minim", "veniam", "quis", "nostrud",
        "exercitation", "ullamco", "laboris", "nisi", "ut", "aliquip", "ex", "ea",
        "commodo", "consequat", "Duis", "aute", "irure", "in", "reprehenderit",
    ];

    for i in 0..num_paragraphs {
        let paragraph_type = match i % 5 {
            0 => ParagraphType::Header1,
            1 => ParagraphType::Header2,
            2 => ParagraphType::Header3,
            3 => ParagraphType::CodeBlock,
            _ => ParagraphType::Text,
        };

        let mut text = String::new();
        for j in 0..avg_words_per_para {
            if j > 0 {
                text.push(' ');
            }
            text.push_str(sample_words[j % sample_words.len()]);
        }

        let paragraph = Paragraph::new(paragraph_type)
            .with_content(vec![Span::new_text(&text)]);
        doc.add_paragraph(paragraph);
    }

    doc
}

/// Create a document with mixed inline styles
fn create_styled_document(num_paragraphs: usize) -> Document {
    let mut doc = Document::new();

    for i in 0..num_paragraphs {
        let text = format!("This is paragraph {} with some bold and italic text and maybe some code.", i);

        // Create spans with different styles
        let span = if i % 15 == 0 {
            // Both bold and italic (nested)
            Span::new_styled(InlineStyle::Bold)
                .with_children(vec![
                    Span::new_styled(InlineStyle::Italic).with_text(&text)
                ])
        } else if i % 3 == 0 {
            Span::new_styled(InlineStyle::Bold).with_text(&text)
        } else if i % 5 == 0 {
            Span::new_styled(InlineStyle::Italic).with_text(&text)
        } else {
            Span::new_text(&text)
        };

        let paragraph = Paragraph::new_text().with_content(vec![span]);
        doc.add_paragraph(paragraph);
    }

    doc
}

struct BenchmarkResult {
    name: String,
    iterations: usize,
    total_duration: Duration,
    avg_duration: Duration,
    min_duration: Duration,
    max_duration: Duration,
}

impl BenchmarkResult {
    fn print(&self) {
        println!("\n{}", "=".repeat(70));
        println!("Benchmark: {}", self.name);
        println!("{}", "=".repeat(70));
        println!("Iterations:     {}", self.iterations);
        println!("Total time:     {:?}", self.total_duration);
        println!("Average:        {:?}", self.avg_duration);
        println!("Min:            {:?}", self.min_duration);
        println!("Max:            {:?}", self.max_duration);
        println!("Ops/sec:        {:.2}", 1_000_000.0 / self.avg_duration.as_micros() as f64);

        // Highlight if performance is concerning
        if self.avg_duration.as_millis() > 100 {
            println!("\n⚠️  WARNING: Average duration > 100ms (user-perceptible lag)");
        } else if self.avg_duration.as_millis() > 16 {
            println!("\n⚠️  WARNING: Average duration > 16ms (may drop frames)");
        }
    }
}

fn benchmark<F>(name: &str, iterations: usize, mut f: F) -> BenchmarkResult
where
    F: FnMut(),
{
    let mut durations = Vec::with_capacity(iterations);

    // Warmup
    for _ in 0..10 {
        f();
    }

    // Actual benchmark
    for _ in 0..iterations {
        let start = Instant::now();
        f();
        durations.push(start.elapsed());
    }

    let total_duration: Duration = durations.iter().sum();
    let avg_duration = total_duration / iterations as u32;
    let min_duration = *durations.iter().min().unwrap();
    let max_duration = *durations.iter().max().unwrap();

    BenchmarkResult {
        name: name.to_string(),
        iterations,
        total_duration,
        avg_duration,
        min_duration,
        max_duration,
    }
}

#[test]
fn bench_rendering_performance() {
    println!("\n\n╔════════════════════════════════════════════════════════════════╗");
    println!("║           RENDERING PERFORMANCE BENCHMARKS                     ║");
    println!("╚════════════════════════════════════════════════════════════════╝");

    let docs = vec![
        ("Small (10 paras)", create_test_document(SMALL_DOC_PARAGRAPHS, 20)),
        ("Medium (100 paras)", create_test_document(MEDIUM_DOC_PARAGRAPHS, 20)),
        ("Large (1000 paras)", create_test_document(LARGE_DOC_PARAGRAPHS, 20)),
        ("Huge (10000 paras)", create_test_document(HUGE_DOC_PARAGRAPHS, 20)),
    ];

    for (name, doc) in docs {
        let result = benchmark(
            &format!("render_document - {}", name),
            if name.contains("Huge") { 10 } else { ITERATIONS },
            || {
                let _ = pure::render::render_document(
                    &doc,
                    80,  // wrap_width
                    0,   // left_padding
                    &[],  // markers
                    &[],  // reveal_tags
                    NO_SENTINELS,
                );
            },
        );
        result.print();
    }
}

#[test]
fn bench_rendering_with_cache() {
    println!("\n\n╔════════════════════════════════════════════════════════════════╗");
    println!("║         RENDERING WITH CACHE BENCHMARKS (CACHE HITS)          ║");
    println!("╚════════════════════════════════════════════════════════════════╝");

    let docs = vec![
        ("Small (10 paras)", create_test_document(SMALL_DOC_PARAGRAPHS, 20)),
        ("Medium (100 paras)", create_test_document(MEDIUM_DOC_PARAGRAPHS, 20)),
        ("Large (1000 paras)", create_test_document(LARGE_DOC_PARAGRAPHS, 20)),
        ("Huge (10000 paras)", create_test_document(HUGE_DOC_PARAGRAPHS, 20)),
    ];

    for (name, doc) in docs {
        let mut cache = pure::render::RenderCache::new();

        // First render - cache miss
        let start = std::time::Instant::now();
        let _ = pure::render::render_document_with_cache(
            &doc,
            80,  // wrap_width
            0,   // left_padding
            &[],  // markers
            &[],  // reveal_tags
            NO_SENTINELS,
            Some(&mut cache),
        );
        let first_render = start.elapsed();

        // Subsequent renders - cache hits
        let iterations = if name.contains("Huge") { 50 } else { 100 };
        let mut durations = Vec::new();

        for _ in 0..iterations {
            let start = std::time::Instant::now();
            let _ = pure::render::render_document_with_cache(
                &doc,
                80,
                0,
                &[],
                &[],
                NO_SENTINELS,
                Some(&mut cache),
            );
            durations.push(start.elapsed());
        }

        let total_duration: Duration = durations.iter().sum();
        let avg_cached = total_duration / iterations as u32;
        let min_cached = *durations.iter().min().unwrap();
        let max_cached = *durations.iter().max().unwrap();

        println!("\n{}", name);
        println!("  First render (cold cache): {:?}", first_render);
        println!("  Cached renders (avg):      {:?}", avg_cached);
        println!("  Cached renders (min):      {:?}", min_cached);
        println!("  Cached renders (max):      {:?}", max_cached);
        println!("  Speedup:                   {:.2}x",
            first_render.as_secs_f64() / avg_cached.as_secs_f64());
        println!("  Cache hits:                {}", cache.hits);
        println!("  Cache misses:              {}", cache.misses);
        println!("  Cache hit rate:            {:.1}%", cache.hit_rate() * 100.0);
    }
}

#[test]
fn bench_rendering_with_styles() {
    println!("\n\n╔════════════════════════════════════════════════════════════════╗");
    println!("║        RENDERING WITH INLINE STYLES BENCHMARKS                 ║");
    println!("╚════════════════════════════════════════════════════════════════╝");

    let docs = vec![
        ("Small styled (10 paras)", create_styled_document(SMALL_DOC_PARAGRAPHS)),
        ("Medium styled (100 paras)", create_styled_document(MEDIUM_DOC_PARAGRAPHS)),
        ("Large styled (1000 paras)", create_styled_document(LARGE_DOC_PARAGRAPHS)),
    ];

    for (name, doc) in docs {
        let result = benchmark(
            &format!("render_document - {}", name),
            ITERATIONS,
            || {
                let _ = pure::render::render_document(
                    &doc,
                    80,
                    0,
                    &[],
                    &[],
                    NO_SENTINELS,
                );
            },
        );
        result.print();
    }
}

#[test]
fn bench_rendering_reveal_codes() {
    println!("\n\n╔════════════════════════════════════════════════════════════════╗");
    println!("║           RENDERING WITH REVEAL CODES BENCHMARKS               ║");
    println!("╚════════════════════════════════════════════════════════════════╝");

    let doc = create_styled_document(MEDIUM_DOC_PARAGRAPHS);

    let result_normal = benchmark(
        "render_document - reveal_codes OFF",
        ITERATIONS,
        || {
            let _ = pure::render::render_document(&doc, 80, 0, &[], &[], NO_SENTINELS);
        },
    );
    result_normal.print();

    let result_reveal = benchmark(
        "render_document - reveal_codes ON",
        ITERATIONS,
        || {
            let _ = pure::render::render_document(&doc, 80, 0, &[], &[], NO_SENTINELS);
        },
    );
    result_reveal.print();

    let overhead_pct = ((result_reveal.avg_duration.as_micros() as f64
                        / result_normal.avg_duration.as_micros() as f64) - 1.0) * 100.0;
    println!("\nReveal codes overhead: {:.1}%", overhead_pct);
}

#[test]
fn bench_segment_collection() {
    println!("\n\n╔════════════════════════════════════════════════════════════════╗");
    println!("║           SEGMENT COLLECTION BENCHMARKS                        ║");
    println!("╚════════════════════════════════════════════════════════════════╝");

    let docs = vec![
        ("Small (10 paras)", create_test_document(SMALL_DOC_PARAGRAPHS, 20)),
        ("Medium (100 paras)", create_test_document(MEDIUM_DOC_PARAGRAPHS, 20)),
        ("Large (1000 paras)", create_test_document(LARGE_DOC_PARAGRAPHS, 20)),
        ("Huge (10000 paras)", create_test_document(HUGE_DOC_PARAGRAPHS, 20)),
    ];

    for (name, doc) in docs {
        let result = benchmark(
            &format!("collect_segments - {}", name),
            if name.contains("Huge") { 10 } else { ITERATIONS },
            || {
                let _ = pure::editor::inspect::collect_segments(&doc, false);
            },
        );
        result.print();

        println!("\n💡 NOTE: This operation runs on EVERY character insertion/deletion!");
    }
}

#[test]
fn bench_segment_collection_with_reveal() {
    println!("\n\n╔════════════════════════════════════════════════════════════════╗");
    println!("║       SEGMENT COLLECTION WITH REVEAL CODES BENCHMARKS          ║");
    println!("╚════════════════════════════════════════════════════════════════╝");

    let doc = create_styled_document(MEDIUM_DOC_PARAGRAPHS);

    let result_normal = benchmark(
        "collect_segments - reveal_codes OFF",
        ITERATIONS,
        || {
            let _ = pure::editor::inspect::collect_segments(&doc, false);
        },
    );
    result_normal.print();

    let result_reveal = benchmark(
        "collect_segments - reveal_codes ON",
        ITERATIONS,
        || {
            let _ = pure::editor::inspect::collect_segments(&doc, true);
        },
    );
    result_reveal.print();
}

#[test]
fn bench_char_to_byte_conversion() {
    println!("\n\n╔════════════════════════════════════════════════════════════════╗");
    println!("║         CHAR-TO-BYTE CONVERSION BENCHMARKS                     ║");
    println!("╚════════════════════════════════════════════════════════════════╝");

    let text_samples = vec![
        ("Short ASCII (50 chars)", "a".repeat(50)),
        ("Medium ASCII (500 chars)", "a".repeat(500)),
        ("Long ASCII (5000 chars)", "a".repeat(5000)),
        ("Short Unicode (50 chars)", "🔥".repeat(50)),
        ("Medium Unicode (500 chars)", "🔥".repeat(500)),
    ];

    for (name, text) in text_samples {
        let char_count = text.chars().count();
        let mid_point = char_count / 2;

        let result = benchmark(
            &format!("char_to_byte_idx (middle of {}) - {}", char_count, name),
            ITERATIONS * 10,
            || {
                let _ = pure::editor::content::char_to_byte_idx(&text, mid_point);
            },
        );
        result.print();

        println!("\n💡 NOTE: This is O(n) and runs multiple times per character insertion!");
    }
}

#[test]
fn bench_word_boundary_detection() {
    println!("\n\n╔════════════════════════════════════════════════════════════════╗");
    println!("║         WORD BOUNDARY DETECTION BENCHMARKS                     ║");
    println!("╚════════════════════════════════════════════════════════════════╝");

    let medium = "lorem ipsum dolor sit amet consectetur adipiscing elit ".repeat(6);
    let long = "lorem ipsum dolor sit amet consectetur adipiscing elit ".repeat(25);
    let text_samples = vec![
        ("Short (10 words)", "lorem ipsum dolor sit amet consectetur adipiscing elit sed do"),
        ("Medium (50 words)", medium.as_str()),
        ("Long (200 words)", long.as_str()),
    ];

    for (name, text) in text_samples {
        let result_prev = benchmark(
            &format!("previous_word_boundary - {}", name),
            ITERATIONS * 10,
            || {
                let _ = pure::editor::content::previous_word_boundary(&text, text.len() / 2);
            },
        );
        result_prev.print();

        let result_next = benchmark(
            &format!("next_word_boundary - {}", name),
            ITERATIONS * 10,
            || {
                let _ = pure::editor::content::next_word_boundary(&text, text.len() / 2);
            },
        );
        result_next.print();

        println!("\n💡 NOTE: Allocates Vec<char> on each call!");
    }
}

#[test]
fn bench_full_edit_cycle() {
    println!("\n\n╔════════════════════════════════════════════════════════════════╗");
    println!("║              FULL EDIT CYCLE BENCHMARKS                        ║");
    println!("╚════════════════════════════════════════════════════════════════╝");
    println!("\nThis simulates the full cost of typing a character:");
    println!("  1. Insert character into document");
    println!("  2. Rebuild segments");
    println!("  3. Re-sync cursor");

    let doc_sizes = vec![
        ("Small (10 paras)", SMALL_DOC_PARAGRAPHS),
        ("Medium (100 paras)", MEDIUM_DOC_PARAGRAPHS),
        ("Large (1000 paras)", LARGE_DOC_PARAGRAPHS),
        ("Huge (10000 paras)", HUGE_DOC_PARAGRAPHS),
    ];

    for (name, size) in doc_sizes {
        let iterations = if name.contains("Huge") { 10 } else { 100 };

        let result = benchmark(
            &format!("Full edit cycle - {}", name),
            iterations,
            || {
                let doc = create_test_document(size, 20);
                let mut editor = pure::editor::DocumentEditor::new(doc);

                // Simulate typing 10 characters
                for _ in 0..10 {
                    editor.insert_char('x');
                }
            },
        );
        result.print();

        // Calculate per-character cost
        let per_char = result.avg_duration / 10;
        println!("\nPer-character cost: {:?}", per_char);

        if per_char.as_millis() > 16 {
            println!("⚠️  CRITICAL: Typing will feel laggy (>16ms per keystroke)");
        } else if per_char.as_millis() > 5 {
            println!("⚠️  WARNING: May feel sluggish on older hardware");
        }
    }
}

#[test]
fn bench_wrap_width_impact() {
    println!("\n\n╔════════════════════════════════════════════════════════════════╗");
    println!("║              WRAP WIDTH IMPACT BENCHMARKS                      ║");
    println!("╚════════════════════════════════════════════════════════════════╝");

    let doc = create_test_document(MEDIUM_DOC_PARAGRAPHS, 50);

    let widths = vec![40, 80, 120, 200];

    for width in widths {
        let result = benchmark(
            &format!("render_document - wrap_width={}", width),
            ITERATIONS,
            || {
                let _ = pure::render::render_document(&doc, width, 0, &[], &[], NO_SENTINELS);
            },
        );
        result.print();
    }
}

#[test]
fn bench_memory_allocations() {
    println!("\n\n╔════════════════════════════════════════════════════════════════╗");
    println!("║         MEMORY ALLOCATION PATTERNS                             ║");
    println!("╚════════════════════════════════════════════════════════════════╝");

    let doc = create_test_document(MEDIUM_DOC_PARAGRAPHS, 20);

    println!("\nDocument stats:");
    println!("  Paragraphs: {}", doc.paragraphs.len());

    let segments = pure::editor::inspect::collect_segments(&doc, false);
    println!("  Segments: {}", segments.len());

    let render_result = pure::render::render_document(&doc, 80, 0, &[], &[], NO_SENTINELS);
    println!("  Rendered lines: {}", render_result.lines.len());
    println!("  Cursor map entries: {}", render_result.cursor_map.len());

    println!("\nAllocations per edit cycle:");
    println!("  - Full segment Vec rebuild");
    println!("  - Cursor map Vec rebuild");
    println!("  - Document clone for render (with sentinel insertion)");
    println!("  - Multiple intermediate Vecs during rendering");
}

#[cfg(test)]
mod summary {
    #[test]
    fn print_summary() {
        println!("\n\n╔════════════════════════════════════════════════════════════════╗");
        println!("║                    BENCHMARK SUMMARY                           ║");
        println!("╚════════════════════════════════════════════════════════════════╝");
        println!("\nTo run all benchmarks:");
        println!("  cargo test --release --bench performance -- --nocapture --test-threads=1");
        println!("\nTo run a specific benchmark:");
        println!("  cargo test --release --bench performance bench_rendering_performance -- --nocapture");
        println!("\nKey metrics to watch:");
        println!("  • Full edit cycle time (should be < 5ms per character)");
        println!("  • Segment collection time (runs on EVERY keystroke)");
        println!("  • Rendering time for large documents");
        println!("  • char_to_byte_idx performance (called multiple times per edit)");
        println!("\nPerformance targets:");
        println!("  • < 16ms per operation = smooth 60 FPS");
        println!("  • < 100ms = user perceives as instantaneous");
        println!("  • > 100ms = noticeable lag");
        println!("  • > 1000ms = unacceptable");
    }
}

#[test]
fn bench_user_guide_rendering() {
    println!("\n\n╔════════════════════════════════════════════════════════════════╗");
    println!("║              USER-GUIDE.MD RENDERING BENCHMARK                 ║");
    println!("╚════════════════════════════════════════════════════════════════╝");

    // Load the actual USER-GUIDE.md
    let content = std::fs::read_to_string("USER-GUIDE.md")
        .expect("Failed to read USER-GUIDE.md");

    let doc = tdoc::markdown::parse(std::io::Cursor::new(&content)).expect("Failed to parse markdown");

    println!("\nDocument stats:");
    println!("  Paragraphs: {}", doc.paragraphs.len());
    println!("  File size: {} bytes", content.len());

    // Test cold rendering (no cache)
    println!("\nCold rendering (no cache):");
    let iterations = 10;
    let mut durations = Vec::new();

    for _ in 0..iterations {
        let start = Instant::now();
        let _ = pure::render::render_document(
            &doc,
            80,
            0,
            &[],
            &[],
            NO_SENTINELS,
        );
        durations.push(start.elapsed());
    }

    let total: Duration = durations.iter().sum();
    let avg = total / iterations as u32;
    let min = *durations.iter().min().unwrap();
    let max = *durations.iter().max().unwrap();

    println!("  Average: {:?}", avg);
    println!("  Min: {:?}", min);
    println!("  Max: {:?}", max);

    if avg.as_millis() > 100 {
        println!("  ⚠️  WARNING: Rendering takes > 100ms (noticeable lag!)");
    } else if avg.as_millis() > 16 {
        println!("  ⚠️  CAUTION: Rendering takes > 16ms (may drop frames)");
    } else {
        println!("  ✅ GOOD: Rendering is fast enough for 60 FPS");
    }

    // Test with cache
    println!("\nWith render cache:");
    let mut cache = pure::render::RenderCache::new();

    // First render - cold cache
    let start = Instant::now();
    let _ = pure::render::render_document_with_cache(
        &doc,
        80,
        0,
        &[],
        &[],
        NO_SENTINELS,
        Some(&mut cache),
    );
    let first_render = start.elapsed();

    // Subsequent renders - warm cache
    let mut cached_durations = Vec::new();
    for _ in 0..iterations {
        let start = Instant::now();
        let _ = pure::render::render_document_with_cache(
            &doc,
            80,
            0,
            &[],
            &[],
            NO_SENTINELS,
            Some(&mut cache),
        );
        cached_durations.push(start.elapsed());
    }

    let cached_total: Duration = cached_durations.iter().sum();
    let cached_avg = cached_total / iterations as u32;

    println!("  First render: {:?}", first_render);
    println!("  Cached average: {:?}", cached_avg);
    println!("  Speedup: {:.2}x", first_render.as_secs_f64() / cached_avg.as_secs_f64());
    println!("  Cache hit rate: {:.1}%", cache.hit_rate() * 100.0);
}
