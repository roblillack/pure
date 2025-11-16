## Pure Refactor Plan

We will migrate Pure to the enum-based `tdoc::Paragraph` API in six incremental steps. Each step isolates a slice of editor/render functionality into its own module, carries along the relevant tests, and gates the refactor behind green checks before moving on.

### 1. Document Inspection & Read-Only Helpers

- **Scope**: All helper routines that only *observe* the document tree (e.g. `paragraph_ref`, `paragraph_mut` wrappers, `text_effective_relation`, `breadcrumbs_for_pointer`, segment collectors, checklist/item inspectors). No mutations or cursor state.
- **Work**:
  - Extract these helpers into a new module, e.g. `src/editor/inspect.rs` (name TBD) with a public API consumed by the rest of the editor.
  - Update call sites in `editor.rs`, `render.rs`, and tests to import from the new module.
  - While moving the code, rewrite it to use the new enum accessors (`paragraph.content()`, `paragraph.children()`, etc.). This confines the enum adaptation to a small, testable surface before touching mutation logic.
- **Tests**:
  - Extract or create unit tests that cover: path traversal, span collection, checklist traversal, and any “effective paragraph” computations.
  - Run `cargo test editor::inspect_tests` (exact mod name TBD) to keep the feedback loop tight, plus `cargo test editor` afterward.

### 2. Cursor & Traversal Logic

- **Scope**: Pointer math, `CursorPointer`, movement helpers (`next_paragraph_path`, `shift_to_next_segment`, etc.), and any functionality that reads the tree to position the caret but does not mutate document content.
- **Work**:
  - Move these functions into `src/editor/cursor.rs`.
  - Depend on the inspection module from Step 1 for read-only tree queries.
  - Update `editor.rs` to delegate cursor operations to the new module; keep `DocumentEditor` as orchestrator only.
  - As part of the move, audit all read-only interactions to the new API (e.g. replace `.children` field lookups) using the helpers from Step 1.
- **Tests**:
  - Port existing cursor traversal tests from `editor_tests.rs` (e.g. navigation, selection) into a dedicated module.
  - Run `cargo test editor::cursor_tests` (or similar) plus the full editor test suite to confirm no regressions.

### 3. Content Mutation Logic (Inline Text / Spans)

- **Scope**: Functions that edit inline text/spans without changing paragraph structure (split paragraphs into siblings is *not* here). Examples: inserting characters, applying inline styles, backspace/delete within a paragraph, span normalization.
- **Work**:
  - Create `src/editor/content.rs` to house helpers for text edits.
  - Ensure these helpers accept explicit paragraph handles (e.g. paths or mutable references) and rely on Step‑1 inspection utilities for traversal.
  - During the move, refactor each function to use the enum API (e.g. `paragraph.content_mut()` instead of `paragraph.content`).
- **Tests**:
  - Move/extend tests that assert text editing correctness, including span merges and revealing tags.
  - Run the new content test module plus the overall suite.

### 4. Paragraph Structure Mutation Logic

- **Scope**: Operations that add/remove/reorder paragraphs or list entries (indent/unindent, promoting children, converting between list/quote types, inserting paragraph breaks, checklist structural edits).
- **Work**:
  - Introduce `src/editor/structure.rs` and migrate all structure-changing helpers.
  - Refactor each helper to operate via explicit enum matches rather than mutating dormant fields. Reuse shared utilities (e.g. `ParagraphBuilder` helpers) to keep logic readable.
  - Ensure Step‑3 content helpers are used for cases that also tweak inline text (e.g. ensuring placeholder spans).
- **Tests**:
  - Move list/indentation/checklist structural tests here.
  - Run the module’s tests and then the entire editor suite after each batch.

### 5. Inline Style Mutation Logic

- **Scope**: Functions that apply/remove inline styles, reveal codes, or manipulate `InlineStyle` metadata.
- **Work**:
  - Extract into `src/editor/styles.rs`.
  - Ensure style helpers use the content module’s primitives for span splitting/merging where needed, thereby reusing enum-aware logic.
  - Update callers (mostly `DocumentEditor` command handlers) to call into this module.
- **Tests**:
  - Port existing inline-style tests, and add coverage specific to the new module.
  - Run targeted tests before rerunning the broader suite.

### 6. Renderer Update

- **Scope**: `src/render.rs`.
- **Work**:
  - Replace all direct field access (`paragraph.content`, `paragraph.children`, `paragraph.entries`, etc.) with the new enum accessors, leveraging any read-only helpers from Step 1 where convenient.
  - While in there, cleanly separate rendering of each `Paragraph` variant so future changes remain localized.
  - Verify marker/reveal handling still aligns with the cursor/content modules extracted earlier.
- **Tests**:
  - Run any renderer-specific tests (if present) and manual smoke tests by running the TUI after `cargo run -- test.ftml`.
  - Finish with `cargo test --all` (workspace) to ensure full integration.

Following this plan keeps the migration incremental: each step lands a compiling, tested module before touching the next layer, reducing the risk of the enum refactor derailing the entire editor.
