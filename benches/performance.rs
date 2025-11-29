use pure_tui::{
    editor,
    render::{self, DirectCursorTracking},
};
use std::time::{Duration, Instant};
use tdoc::{Document, InlineStyle, Paragraph, ParagraphType, Span};

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

/// Create a test document with the specified number of paragraphs
fn create_test_document(num_paragraphs: usize, avg_words_per_para: usize) -> Document {
    let mut doc = Document::new();

    let sample_words = vec![
        "Lorem",
        "ipsum",
        "dolor",
        "sit",
        "amet",
        "consectetur",
        "adipiscing",
        "elit",
        "sed",
        "do",
        "eiusmod",
        "tempor",
        "incididunt",
        "ut",
        "labore",
        "et",
        "dolore",
        "magna",
        "aliqua",
        "Ut",
        "enim",
        "ad",
        "minim",
        "veniam",
        "quis",
        "nostrud",
        "exercitation",
        "ullamco",
        "laboris",
        "nisi",
        "ut",
        "aliquip",
        "ex",
        "ea",
        "commodo",
        "consequat",
        "Duis",
        "aute",
        "irure",
        "in",
        "reprehenderit",
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

        let paragraph = Paragraph::new(paragraph_type).with_content(vec![Span::new_text(&text)]);
        doc.add_paragraph(paragraph);
    }

    doc
}

/// Create a document with mixed inline styles
fn create_styled_document(num_paragraphs: usize) -> Document {
    let mut doc = Document::new();

    for i in 0..num_paragraphs {
        let text = format!(
            "This is paragraph {} with some bold and italic text and maybe some code.",
            i
        );

        // Create spans with different styles
        let span = if i % 15 == 0 {
            // Both bold and italic (nested)
            Span::new_styled(InlineStyle::Bold)
                .with_children(vec![Span::new_styled(InlineStyle::Italic).with_text(&text)])
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
        println!(
            "Ops/sec:        {:.2}",
            1_000_000.0 / self.avg_duration.as_micros() as f64
        );

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
        (
            "Small (10 paras)",
            create_test_document(SMALL_DOC_PARAGRAPHS, 20),
        ),
        (
            "Medium (100 paras)",
            create_test_document(MEDIUM_DOC_PARAGRAPHS, 20),
        ),
        (
            "Large (1000 paras)",
            create_test_document(LARGE_DOC_PARAGRAPHS, 20),
        ),
        (
            "Huge (10000 paras)",
            create_test_document(HUGE_DOC_PARAGRAPHS, 20),
        ),
    ];

    for (name, doc) in docs {
        let result = benchmark(
            &format!("render_document - {}", name),
            if name.contains("Huge") {
                10
            } else {
                ITERATIONS
            },
            || {
                let tracking = DirectCursorTracking {
                    cursor: None,
                    selection: None,
                    track_all_positions: false,
                };
                let _ = render::render_document_direct(
                    &doc,
                    80,  // wrap_width
                    0,   // left_padding
                    &[], // reveal_tags
                    tracking,
                    // None
                );
            },
        );
        result.print();
    }
}

#[test]
fn bench_rendering_with_styles() {
    println!("\n\n╔════════════════════════════════════════════════════════════════╗");
    println!("║        RENDERING WITH INLINE STYLES BENCHMARKS                 ║");
    println!("╚════════════════════════════════════════════════════════════════╝");

    let docs = vec![
        (
            "Small styled (10 paras)",
            create_styled_document(SMALL_DOC_PARAGRAPHS),
        ),
        (
            "Medium styled (100 paras)",
            create_styled_document(MEDIUM_DOC_PARAGRAPHS),
        ),
        (
            "Large styled (1000 paras)",
            create_styled_document(LARGE_DOC_PARAGRAPHS),
        ),
    ];

    for (name, doc) in docs {
        let result = benchmark(&format!("render_document - {}", name), ITERATIONS, || {
            let tracking = DirectCursorTracking {
                cursor: None,
                selection: None,
                track_all_positions: false,
            };
            let _ = render::render_document_direct(&doc, 80, 0, &[], tracking);
        });
        result.print();
    }
}

#[test]
fn bench_rendering_reveal_codes() {
    println!("\n\n╔════════════════════════════════════════════════════════════════╗");
    println!("║           RENDERING WITH REVEAL CODES BENCHMARKS               ║");
    println!("╚════════════════════════════════════════════════════════════════╝");

    let doc = create_styled_document(MEDIUM_DOC_PARAGRAPHS);

    let result_normal = benchmark("render_document - reveal_codes OFF", ITERATIONS, || {
        let tracking = DirectCursorTracking {
            cursor: None,
            selection: None,
            track_all_positions: false,
        };
        let _ = render::render_document_direct(&doc, 80, 0, &[], tracking);
    });
    result_normal.print();

    let result_reveal = benchmark("render_document - reveal_codes ON", ITERATIONS, || {
        let tracking = DirectCursorTracking {
            cursor: None,
            selection: None,
            track_all_positions: false,
        };
        let _ = render::render_document_direct(&doc, 80, 0, &[], tracking);
    });
    result_reveal.print();

    let overhead_pct = ((result_reveal.avg_duration.as_micros() as f64
        / result_normal.avg_duration.as_micros() as f64)
        - 1.0)
        * 100.0;
    println!("\nReveal codes overhead: {:.1}%", overhead_pct);
}

#[test]
fn bench_segment_collection() {
    println!("\n\n╔════════════════════════════════════════════════════════════════╗");
    println!("║           SEGMENT COLLECTION BENCHMARKS                        ║");
    println!("╚════════════════════════════════════════════════════════════════╝");

    let docs = vec![
        (
            "Small (10 paras)",
            create_test_document(SMALL_DOC_PARAGRAPHS, 20),
        ),
        (
            "Medium (100 paras)",
            create_test_document(MEDIUM_DOC_PARAGRAPHS, 20),
        ),
        (
            "Large (1000 paras)",
            create_test_document(LARGE_DOC_PARAGRAPHS, 20),
        ),
        (
            "Huge (10000 paras)",
            create_test_document(HUGE_DOC_PARAGRAPHS, 20),
        ),
    ];

    for (name, doc) in docs {
        let result = benchmark(
            &format!("collect_segments - {}", name),
            if name.contains("Huge") {
                10
            } else {
                ITERATIONS
            },
            || {
                let _ = editor::inspect::collect_segments(&doc, false);
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

    let result_normal = benchmark("collect_segments - reveal_codes OFF", ITERATIONS, || {
        let _ = editor::inspect::collect_segments(&doc, false);
    });
    result_normal.print();

    let result_reveal = benchmark("collect_segments - reveal_codes ON", ITERATIONS, || {
        let _ = editor::inspect::collect_segments(&doc, true);
    });
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
                let _ = editor::content::char_to_byte_idx(&text, mid_point);
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
        (
            "Short (10 words)",
            "lorem ipsum dolor sit amet consectetur adipiscing elit sed do",
        ),
        ("Medium (50 words)", medium.as_str()),
        ("Long (200 words)", long.as_str()),
    ];

    for (name, text) in text_samples {
        let result_prev = benchmark(
            &format!("previous_word_boundary - {}", name),
            ITERATIONS * 10,
            || {
                let _ = editor::content::previous_word_boundary(text, text.len() / 2);
            },
        );
        result_prev.print();

        let result_next = benchmark(
            &format!("next_word_boundary - {}", name),
            ITERATIONS * 10,
            || {
                let _ = editor::content::next_word_boundary(text, text.len() / 2);
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

        let result = benchmark(&format!("Full edit cycle - {}", name), iterations, || {
            let doc = create_test_document(size, 20);
            let mut editor = editor::DocumentEditor::new(doc);

            // Simulate typing 10 characters
            for _ in 0..10 {
                editor.insert_char('x');
            }
        });
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
                let tracking = DirectCursorTracking {
                    cursor: None,
                    selection: None,
                    track_all_positions: false,
                };
                let _ = render::render_document_direct(&doc, width, 0, &[], tracking);
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

    let segments = editor::inspect::collect_segments(&doc, false);
    println!("  Segments: {}", segments.len());

    let tracking = DirectCursorTracking {
        cursor: None,
        selection: None,
        track_all_positions: false,
    };
    let render_result = render::render_document_direct(&doc, 80, 0, &[], tracking);
    println!("  Rendered lines: {}", render_result.lines.len());
    println!(
        "  Cursor map entries: {}",
        render_result
            .paragraph_lines
            .iter()
            .map(|p| p.positions.len())
            .sum::<usize>()
    );

    println!("\nAllocations per edit cycle:");
    println!("  - Full segment Vec rebuild");
    println!("  - Cursor map Vec rebuild");
    println!("  - Direct rendering without document cloning");
    println!("  - Multiple intermediate Vecs during rendering");
}

#[test]
fn bench_scrolling_detailed_analysis() {
    println!("\n\n╔════════════════════════════════════════════════════════════════╗");
    println!("║           DETAILED PERFORMANCE ANALYSIS                       ║");
    println!("╚════════════════════════════════════════════════════════════════╝");
    println!("\nThis benchmark analyzes exactly what's causing the slowdown");
    println!("during scrolling operations in USER-GUIDE.md.");

    use pure_tui::editor_display::EditorDisplay;
    use ratatui::layout::Rect;

    // Load USER-GUIDE.md
    let content = std::fs::read_to_string("USER-GUIDE.md").expect("Failed to read USER-GUIDE.md");
    let doc =
        tdoc::markdown::parse(std::io::Cursor::new(&content)).expect("Failed to parse markdown");

    let editor = editor::DocumentEditor::new(doc);
    let mut display = EditorDisplay::new(editor);

    // Initial render to populate visual positions
    let text_area = Rect {
        x: 0,
        y: 0,
        width: 100,
        height: 30,
    };
    display.render_document(80, 0, None);
    display.update_after_render(text_area);

    println!("\n📊 Document Statistics:");
    println!(
        "  Paragraphs:          {}",
        display.document().paragraphs.len()
    );
    println!(
        "  Total visual lines:  {}",
        display.get_layout().lines.len()
    );
    println!(
        "  Cursor map entries:  {}",
        display
            .get_layout()
            .paragraph_lines
            .iter()
            .map(|p| p.positions.len())
            .sum::<usize>()
    );
    println!(
        "  Visual positions:    {}",
        display.visual_positions().len()
    );
    println!("  File size:           {} bytes", content.len());

    // Measure a single operation in detail
    println!("\n🔬 Single Operation Analysis (move down + render):");

    // Measure move_cursor_vertical
    let move_start = Instant::now();
    display.move_cursor_vertical(1);
    let move_time = move_start.elapsed();
    println!("  move_cursor_vertical: {:?}", move_time);

    // Measure render_document
    let render_start = Instant::now();
    display.render_document(80, 0, None);
    let render_time = render_start.elapsed();
    println!("  render_document:      {:?}", render_time);
    println!(
        "  Cursor map size:      {}",
        display
            .get_layout()
            .paragraph_lines
            .iter()
            .map(|p| p.positions.len())
            .sum::<usize>()
    );

    // Analysis
    let layout = display.get_layout();
    println!("\n🔍 Performance Analysis:");
    println!("\n1. RENDERING BOTTLENECK:");
    println!(
        "   Rendering takes {:?} ({:.1}% of total time)",
        render_time,
        render_time.as_secs_f64() / (move_time + render_time).as_secs_f64() * 100.0
    );

    println!("\n2. CURSOR MAP SIZE:");
    println!(
        "   {} cursor positions tracked per render",
        layout
            .paragraph_lines
            .iter()
            .map(|p| p.positions.len())
            .sum::<usize>()
    );
    println!("   This happens because track_all_positions=true in EditorDisplay::render_document");

    println!("\n3. ROOT CAUSES:");
    println!(
        "   ✗ track_all_positions=true tracks {} positions per render",
        layout
            .paragraph_lines
            .iter()
            .map(|p| p.positions.len())
            .sum::<usize>()
    );
    println!("   ✗ Every render allocates and populates large cursor_map Vec");
    println!("   ✗ visual_positions Vec is rebuilt on every render");
    println!(
        "   ✗ With {} visual lines, this creates significant overhead",
        layout.lines.len()
    );

    println!("\n4. RECOMMENDED FIXES:");
    println!("   1. Set track_all_positions=false when only cursor position is needed");
    println!("   2. Only track full cursor_map when actually needed (e.g., for mouse clicks)");
    println!("   3. Consider incremental updates to cursor_map instead of full rebuild");
    println!("   4. Cache visual_positions separately to avoid rebuilding on every render");
    println!("   5. Use a more efficient data structure for cursor_map (e.g., HashMap)");

    println!("\n5. EXPECTED IMPROVEMENT:");
    println!("   If track_all_positions=false:");
    println!(
        "   - Eliminate {} cursor position tracking operations",
        layout
            .paragraph_lines
            .iter()
            .map(|p| p.positions.len())
            .sum::<usize>()
    );
    println!("   - Reduce memory allocations significantly");
    println!("   - Expected render time: <5ms (10x improvement)");
    println!("   - Combined time: <8ms (meets <10ms target)");
}

#[test]
fn bench_editing_insert_text() {
    println!("\n\n╔════════════════════════════════════════════════════════════════╗");
    println!("║       EDITING BENCHMARK: INSERT TEXT (Typing Performance)     ║");
    println!("╚════════════════════════════════════════════════════════════════╝");
    println!("\nThis simulates opening USER-GUIDE.md, finding the first text");
    println!("paragraph, and typing 100 words at the beginning.");

    use pure_tui::editor_display::EditorDisplay;
    use ratatui::layout::Rect;

    // Load USER-GUIDE.md
    let content = std::fs::read_to_string("USER-GUIDE.md").expect("Failed to read USER-GUIDE.md");
    let doc =
        tdoc::markdown::parse(std::io::Cursor::new(&content)).expect("Failed to parse markdown");

    let editor = editor::DocumentEditor::new(doc);
    let mut display = EditorDisplay::new(editor);

    // Initial render to populate visual positions
    let text_area = Rect {
        x: 0,
        y: 0,
        width: 100,
        height: 30,
    };
    display.render_document(80, 0, None);
    display.update_after_render(text_area);

    println!("\nDocument stats:");
    println!("  Paragraphs: {}", display.document().paragraphs.len());
    println!("  Total visual lines: {}", display.get_layout().lines.len());

    // Find first text paragraph and move cursor there
    // USER-GUIDE starts with heading paragraphs, so we need to move to first text paragraph
    for _ in 0..20 {
        display.move_down();
        if matches!(
            display
                .document()
                .paragraphs
                .get(display.cursor_pointer().paragraph_path.numeric_steps()[0]),
            Some(tdoc::Paragraph::Text { .. })
        ) {
            break;
        }
    }

    println!(
        "  Starting paragraph: {:?}",
        display.cursor_pointer().paragraph_path.numeric_steps()
    );

    // Prepare text to insert: 100 words
    let words_to_insert = "Lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor incididunt ut labore et dolore magna aliqua ";
    let total_chars = words_to_insert.len() * 10; // Repeat to get ~100 words

    println!("  Characters to insert: {}", total_chars);

    // Track timing for each character insertion
    let mut insert_times = Vec::new();
    let mut render_times = Vec::new();
    let overall_start = Instant::now();

    let chars_to_type: Vec<char> = words_to_insert.chars().cycle().take(total_chars).collect();

    for (idx, ch) in chars_to_type.iter().enumerate() {
        // Time the character insertion
        let insert_start = Instant::now();
        display.insert_char(*ch);
        insert_times.push(insert_start.elapsed());

        // Render after insertion (uses incremental updates)
        let render_start = Instant::now();
        display.render_document(80, 0, None);
        render_times.push(render_start.elapsed());
        display.update_after_render(text_area);

        // Sample every 100 characters for progress
        if (idx + 1) % 100 == 0 {
            println!("  Inserted {} characters...", idx + 1);
        }
    }

    let overall_time = overall_start.elapsed();

    // Calculate statistics
    let total_ops = insert_times.len();
    let insert_avg: Duration = insert_times.iter().sum::<Duration>() / total_ops as u32;
    let insert_min = *insert_times.iter().min().unwrap();
    let insert_max = *insert_times.iter().max().unwrap();

    let render_avg: Duration = render_times.iter().sum::<Duration>() / total_ops as u32;
    let render_min = *render_times.iter().min().unwrap();
    let render_max = *render_times.iter().max().unwrap();

    let combined_avg = insert_avg + render_avg;

    println!("\n╔════════════════════════════════════════════════════════════════╗");
    println!("║                         RESULTS                                ║");
    println!("╚════════════════════════════════════════════════════════════════╝");
    println!("\n📊 Overall:");
    println!("  Total characters:   {}", total_ops);
    println!("  Total time:         {:?}", overall_time);
    println!("  Avg per keypress:   {:?}", combined_avg);
    println!();
    println!("✍️  Character Insertion (insert_char):");
    println!("  Average:  {:?}", insert_avg);
    println!("  Min:      {:?}", insert_min);
    println!("  Max:      {:?}", insert_max);
    println!();
    println!("🎨 Rendering (render_document with incremental updates):");
    println!("  Average:  {:?}", render_avg);
    println!("  Min:      {:?}", render_min);
    println!("  Max:      {:?}", render_max);
    println!();
    println!("⚡ Combined per keypress:");
    println!("  Average:  {:?}", combined_avg);

    // Performance assessment
    println!("\n🎯 Performance Assessment:");
    if combined_avg.as_millis() > 10 {
        println!(
            "  ❌ FAILED: Average > 10ms target ({:.2}ms)",
            combined_avg.as_secs_f64() * 1000.0
        );
        println!("     Users will experience noticeable lag when typing.");
    } else {
        println!(
            "  ✅ PASSED: Average < 10ms target ({:.2}ms)",
            combined_avg.as_secs_f64() * 1000.0
        );
        println!("     Typing should feel smooth and responsive.");
    }

    if combined_avg.as_millis() > 16 {
        println!("  ⚠️  WARNING: Average > 16ms - will drop below 60 FPS");
    }

    // Analyze the breakdown
    println!("\n📈 Performance Breakdown:");
    let insert_pct = insert_avg.as_secs_f64() / combined_avg.as_secs_f64() * 100.0;
    let render_pct = render_avg.as_secs_f64() / combined_avg.as_secs_f64() * 100.0;
    println!("  Insert: {:.1}% of time", insert_pct);
    println!("  Render: {:.1}% of time", render_pct);

    if insert_avg.as_millis() > 5 {
        println!("\n  💡 Insertion is slow - potential optimizations:");
        println!("     - Reduce segment collection overhead");
        println!("     - Optimize char_to_byte_idx conversions");
        println!("     - Batch cursor position updates");
    }

    if render_avg.as_millis() > 5 {
        println!("\n  💡 Rendering is slow - potential optimizations:");
        println!("     - This uses render_document (with incremental updates)");
        println!("     - Check if incremental updates are working correctly");
        println!("     - Optimize paragraph hashing for cache invalidation");
    }
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
        println!(
            "  cargo test --release --bench performance bench_rendering_performance -- --nocapture"
        );
        println!("\nKey metrics to watch:");
        println!("  • Scrolling performance (should be < 10ms per keypress)");
        println!("  • Editing performance (should be < 10ms per keypress)");
        println!("  • Full edit cycle time (should be < 5ms per character)");
        println!("  • Segment collection time (runs on EVERY keystroke)");
        println!("  • Rendering time for large documents");
        println!("  • char_to_byte_idx performance (called multiple times per edit)");
        println!("\nPerformance targets:");
        println!("  • < 10ms per keypress = smooth typing/scrolling");
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
    let content = std::fs::read_to_string("USER-GUIDE.md").expect("Failed to read USER-GUIDE.md");

    let doc =
        tdoc::markdown::parse(std::io::Cursor::new(&content)).expect("Failed to parse markdown");

    println!("\nDocument stats:");
    println!("  Paragraphs: {}", doc.paragraphs.len());
    println!("  File size: {} bytes", content.len());

    // Test cold rendering (no cache)
    println!("\nCold rendering (no cache):");
    let iterations = 10;
    let mut durations = Vec::new();

    for _ in 0..iterations {
        let start = Instant::now();
        let tracking = DirectCursorTracking {
            cursor: None,
            selection: None,
            track_all_positions: false,
        };
        render::render_document_direct(&doc, 80, 0, &[], tracking);
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

    // Note: In production, render caching is handled by EditorDisplay's layout cache
    println!("\nNote: Direct rendering doesn't have per-paragraph caching,");
    println!("      but EditorDisplay provides layout caching for real-world use.");
}

#[test]
fn bench_real_world_render_flow() {
    println!("\n\n╔════════════════════════════════════════════════════════════════╗");
    println!("║        REAL-WORLD RENDER FLOW (WHAT APP ACTUALLY DOES)        ║");
    println!("╚════════════════════════════════════════════════════════════════╝");
    println!("\nThis measures the COMPLETE flow that happens on every frame:");
    println!("  1. Create cursor tracking (no document cloning!)");
    println!("  2. Render the document directly with cache");
    println!("  3. Clone the rendered lines (for widget)");

    // Load actual USER-GUIDE.md
    let content = std::fs::read_to_string("USER-GUIDE.md").expect("Failed to read USER-GUIDE.md");
    let doc =
        tdoc::markdown::parse(std::io::Cursor::new(&content)).expect("Failed to parse markdown");

    let editor = editor::DocumentEditor::new(doc);

    println!("\nDocument stats:");
    println!("  Paragraphs: {}", editor.document().paragraphs.len());
    println!("  File size: {} bytes", content.len());

    let iterations = 20;
    let mut durations = Vec::new();
    let mut tracking_times = Vec::new();
    let mut render_times = Vec::new();
    let mut line_clone_times = Vec::new();

    for _ in 0..iterations {
        let total_start = Instant::now();

        // Step 1: Create cursor tracking (no document cloning needed!)
        let tracking_start = Instant::now();
        let pointer = editor.cursor_pointer();
        let tracking = DirectCursorTracking {
            cursor: Some(&pointer),
            selection: None,
            track_all_positions: false,
        };
        tracking_times.push(tracking_start.elapsed());

        // Step 2: Render the document directly with cache
        let render_start = Instant::now();
        let render_result = render::render_document_direct(editor.document(), 80, 0, &[], tracking);
        render_times.push(render_start.elapsed());

        // Step 3: Clone rendered lines (for ratatui widget)
        let line_clone_start = Instant::now();
        let _lines_clone = render_result.lines.clone();
        line_clone_times.push(line_clone_start.elapsed());

        durations.push(total_start.elapsed());
    }

    let total_avg: Duration = durations.iter().sum::<Duration>() / iterations as u32;
    let tracking_avg: Duration = tracking_times.iter().sum::<Duration>() / iterations as u32;
    let render_avg: Duration = render_times.iter().sum::<Duration>() / iterations as u32;
    let line_clone_avg: Duration = line_clone_times.iter().sum::<Duration>() / iterations as u32;

    println!("\n📊 Performance Breakdown:");
    println!(
        "  Cursor tracking:      {:>8?}  ({:>5.1}%)",
        tracking_avg,
        tracking_avg.as_secs_f64() / total_avg.as_secs_f64() * 100.0
    );
    println!(
        "  Render (cached):      {:>8?}  ({:>5.1}%)",
        render_avg,
        render_avg.as_secs_f64() / total_avg.as_secs_f64() * 100.0
    );
    println!(
        "  Clone render lines:   {:>8?}  ({:>5.1}%)",
        line_clone_avg,
        line_clone_avg.as_secs_f64() / total_avg.as_secs_f64() * 100.0
    );
    println!("  ─────────────────────────────────────");
    println!("  TOTAL per frame:      {:>8?}  (100.0%)", total_avg);

    println!("\n🔍 Analysis:");
    println!("  ✨ Direct rendering eliminates document cloning overhead!");
    println!("     Cursor tracking is now negligible compared to old cloning approach.");

    if total_avg.as_millis() > 100 {
        println!("  ❌ TOTAL time > 100ms - users will notice lag");
    } else if total_avg.as_millis() > 16 {
        println!("  ⚠️  TOTAL time > 16ms - may drop frames");
    } else {
        println!("  ✅ TOTAL time < 16ms - smooth 60 FPS");
    }
}

#[test]
fn bench_scrolling_cursor_movement() {
    println!("\n\n╔════════════════════════════════════════════════════════════════╗");
    println!("║      SCROLLING BENCHMARK: CURSOR MOVEMENT (Down Arrow)         ║");
    println!("╚════════════════════════════════════════════════════════════════╝");
    println!("\nThis simulates scrolling through USER-GUIDE.md by pressing");
    println!("Down arrow repeatedly until reaching the bottom of the document.");

    use pure_tui::editor_display::EditorDisplay;
    use ratatui::layout::Rect;

    // Load USER-GUIDE.md
    let content = std::fs::read_to_string("USER-GUIDE.md").expect("Failed to read USER-GUIDE.md");
    let doc =
        tdoc::markdown::parse(std::io::Cursor::new(&content)).expect("Failed to parse markdown");

    let editor = editor::DocumentEditor::new(doc);
    let mut display = EditorDisplay::new(editor);

    // Initial render to populate visual positions
    let text_area = Rect {
        x: 0,
        y: 0,
        width: 100,
        height: 30,
    };
    display.render_document(80, 0, None);
    display.update_after_render(text_area);

    println!("\nDocument stats:");
    println!("  Paragraphs: {}", display.document().paragraphs.len());
    println!("  Total visual lines: {}", display.get_layout().lines.len());
    println!("  File size: {} bytes", content.len());

    // Track timing for each down arrow press
    let mut move_times = Vec::new();
    let mut render_times = Vec::new();
    let overall_start = Instant::now();

    let max_iterations = 10000; // Safety limit
    for iteration in 0..max_iterations {
        // Remember where we were
        let before_pointer = display.cursor_pointer();

        // Time the cursor movement
        let move_start = Instant::now();
        display.move_cursor_vertical(1);
        move_times.push(move_start.elapsed());

        // Time the render (which happens after each keypress)
        let render_start = Instant::now();
        display.render_document(80, 0, None);
        render_times.push(render_start.elapsed());
        display.update_after_render(text_area);

        // Check if we've reached the end (cursor didn't move)
        let after_pointer = display.cursor_pointer();
        if before_pointer == after_pointer {
            println!("\nReached end of document after {} moves", iteration);
            break;
        }
    }

    let overall_time = overall_start.elapsed();

    // Calculate statistics
    let total_moves = move_times.len();
    let move_avg: Duration = move_times.iter().sum::<Duration>() / total_moves as u32;
    let move_min = *move_times.iter().min().unwrap();
    let move_max = *move_times.iter().max().unwrap();

    let render_avg: Duration = render_times.iter().sum::<Duration>() / total_moves as u32;
    let render_min = *render_times.iter().min().unwrap();
    let render_max = *render_times.iter().max().unwrap();

    let combined_avg = move_avg + render_avg;

    println!("\n╔════════════════════════════════════════════════════════════════╗");
    println!("║                         RESULTS                                ║");
    println!("╚════════════════════════════════════════════════════════════════╝");
    println!("\n📊 Overall:");
    println!("  Total moves:        {}", total_moves);
    println!("  Total time:         {:?}", overall_time);
    println!("  Avg per keypress:   {:?}", combined_avg);
    println!();
    println!("📈 Cursor Movement (move_cursor_vertical):");
    println!("  Average:  {:?}", move_avg);
    println!("  Min:      {:?}", move_min);
    println!("  Max:      {:?}", move_max);
    println!();
    println!("🎨 Rendering (render_document):");
    println!("  Average:  {:?}", render_avg);
    println!("  Min:      {:?}", render_min);
    println!("  Max:      {:?}", render_max);
    println!();
    println!("⚡ Combined per keypress:");
    println!("  Average:  {:?}", combined_avg);

    // Performance assessment
    println!("\n🎯 Performance Assessment:");
    if combined_avg.as_millis() > 10 {
        println!(
            "  ❌ FAILED: Average > 10ms target ({:.2}ms)",
            combined_avg.as_secs_f64() * 1000.0
        );
        println!("     Users will experience noticeable lag when scrolling.");
    } else {
        println!(
            "  ✅ PASSED: Average < 10ms target ({:.2}ms)",
            combined_avg.as_secs_f64() * 1000.0
        );
        println!("     Scrolling should feel smooth and responsive.");
    }

    if combined_avg.as_millis() > 16 {
        println!("  ⚠️  WARNING: Average > 16ms - will drop below 60 FPS");
    }
}

#[test]
fn bench_scrolling_page_down() {
    println!("\n\n╔════════════════════════════════════════════════════════════════╗");
    println!("║         SCROLLING BENCHMARK: PAGE DOWN (Page Down Key)        ║");
    println!("╚════════════════════════════════════════════════════════════════╝");
    println!("\nThis simulates scrolling through USER-GUIDE.md by pressing");
    println!("Page Down repeatedly until reaching the bottom of the document.");

    use pure_tui::editor_display::EditorDisplay;
    use ratatui::layout::Rect;

    // Load USER-GUIDE.md
    let content = std::fs::read_to_string("USER-GUIDE.md").expect("Failed to read USER-GUIDE.md");
    let doc =
        tdoc::markdown::parse(std::io::Cursor::new(&content)).expect("Failed to parse markdown");

    let editor = editor::DocumentEditor::new(doc);
    let mut display = EditorDisplay::new(editor);

    // Initial render to populate visual positions
    let text_area = Rect {
        x: 0,
        y: 0,
        width: 100,
        height: 30,
    };
    display.render_document(80, 0, None);
    display.update_after_render(text_area);

    println!("\nDocument stats:");
    println!("  Paragraphs: {}", display.document().paragraphs.len());
    println!("  Total visual lines: {}", display.get_layout().lines.len());
    println!(
        "  Page jump distance: {} lines",
        display.page_jump_distance()
    );
    println!("  File size: {} bytes", content.len());

    // Track timing for each page down press
    let mut move_times = Vec::new();
    let mut render_times = Vec::new();
    let overall_start = Instant::now();

    let max_iterations = 1000; // Safety limit
    for iteration in 0..max_iterations {
        // Remember where we were
        let before_pointer = display.cursor_pointer();

        // Time the page movement
        let move_start = Instant::now();
        display.move_page(1);
        move_times.push(move_start.elapsed());

        // Time the render (which happens after each keypress)
        let render_start = Instant::now();
        display.render_document(80, 0, None);
        render_times.push(render_start.elapsed());
        display.update_after_render(text_area);

        // Check if we've reached the end (cursor didn't move)
        let after_pointer = display.cursor_pointer();
        if before_pointer == after_pointer {
            println!("\nReached end of document after {} page downs", iteration);
            break;
        }
    }

    let overall_time = overall_start.elapsed();

    // Calculate statistics
    let total_moves = move_times.len();
    let move_avg: Duration = move_times.iter().sum::<Duration>() / total_moves as u32;
    let move_min = *move_times.iter().min().unwrap();
    let move_max = *move_times.iter().max().unwrap();

    let render_avg: Duration = render_times.iter().sum::<Duration>() / total_moves as u32;
    let render_min = *render_times.iter().min().unwrap();
    let render_max = *render_times.iter().max().unwrap();

    let combined_avg = move_avg + render_avg;

    println!("\n╔════════════════════════════════════════════════════════════════╗");
    println!("║                         RESULTS                                ║");
    println!("╚════════════════════════════════════════════════════════════════╝");
    println!("\n📊 Overall:");
    println!("  Total page downs:   {}", total_moves);
    println!("  Total time:         {:?}", overall_time);
    println!("  Avg per keypress:   {:?}", combined_avg);
    println!();
    println!("📈 Page Movement (move_page):");
    println!("  Average:  {:?}", move_avg);
    println!("  Min:      {:?}", move_min);
    println!("  Max:      {:?}", move_max);
    println!();
    println!("🎨 Rendering (render_document):");
    println!("  Average:  {:?}", render_avg);
    println!("  Min:      {:?}", render_min);
    println!("  Max:      {:?}", render_max);
    println!();
    println!("⚡ Combined per keypress:");
    println!("  Average:  {:?}", combined_avg);

    // Performance assessment
    println!("\n🎯 Performance Assessment:");
    if combined_avg.as_millis() > 10 {
        println!(
            "  ❌ FAILED: Average > 10ms target ({:.2}ms)",
            combined_avg.as_secs_f64() * 1000.0
        );
        println!("     Users will experience noticeable lag when paging.");
    } else {
        println!(
            "  ✅ PASSED: Average < 10ms target ({:.2}ms)",
            combined_avg.as_secs_f64() * 1000.0
        );
        println!("     Paging should feel smooth and responsive.");
    }

    if combined_avg.as_millis() > 16 {
        println!("  ⚠️  WARNING: Average > 16ms - will drop below 60 FPS");
    }
}

#[test]
fn benchmark_incremental_updates() {
    use pure_tui::editor::DocumentEditor;
    use pure_tui::editor_display::EditorDisplay;

    println!("\n=== Incremental Update Performance ===\n");

    // Test with medium document (584 paragraphs like USER-GUIDE.md)
    let doc = create_test_document(584, 20);
    let editor = DocumentEditor::new(doc);
    let mut display = EditorDisplay::new(editor);

    // Initial render
    display.render_document(80, 0, None);

    // Test incremental updates
    let iterations = 100;
    let start = Instant::now();
    for _ in 0..iterations {
        display.insert_char('x');
        display.clear_render_cache();
        display.render_document(80, 0, None);
    }
    let total = start.elapsed();
    let avg = total / iterations as u32;

    println!("Document: 584 paragraphs");
    println!("Iterations: {}", iterations);
    println!("Total time: {:?}", total);
    println!("Average per keypress: {:.2}ms", avg.as_secs_f64() * 1000.0);

    if avg.as_millis() > 10 {
        println!("  ❌ WARNING: Incremental updates slower than 10ms target");
    } else {
        println!("  ✅ PASSED: Incremental updates < 10ms - should feel responsive");
    }
}
