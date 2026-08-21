//! Spell checking: dictionary loading, word tokenization, and document scans.
//!
//! Checking is delegated to [`spellbook`], a pure-Rust checker for Hunspell's
//! `.aff`/`.dic` dictionary pair — so Pure needs no C library and no system
//! spell checker. The American English dictionary is embedded in the binary
//! (see `dictionaries/`), and other languages come from the user's own
//! `~/.config/pure/dictionaries` directory, `$DICPATH`, or the system
//! dictionary directories.
//!
//! This module knows nothing about the UI: it turns a [`Document`] into a list
//! of [`Misspelling`]s (leaf path plus byte range, the same coordinates the
//! editor's [`rutle::DocumentPosition`] uses) and offers suggestions for a
//! word. The modal dialog that walks that list lives in
//! [`crate::spell_dialog`].

use std::collections::HashSet;
use std::env;
use std::fmt;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use rutle::DocumentPosition;
use rutle::tree_path::TreePath;
use rutle::tree_walk::{self, ParaKind};
use spellbook::Dictionary;
use tdoc::{Document, InlineStyle, Span};

use crate::config::Config;

/// The dictionary compiled into the binary (default `bundled-dictionary`
/// feature): SCOWL-derived American English, see `dictionaries/README.md`.
#[cfg(feature = "bundled-dictionary")]
mod bundled {
    pub const LANGUAGE: &str = "en_US";
    pub const AFF: &str = include_str!("../dictionaries/en_US/en_US.aff");
    pub const DIC: &str = include_str!("../dictionaries/en_US/en_US.dic");
}

/// Suggestions offered for one misspelling. Hunspell's ngram phase can produce
/// long tails of increasingly unlikely words; the dialog only ever shows a
/// handful, so cap the list where it's cheap.
const MAX_SUGGESTIONS: usize = 8;

/// Words shorter than this are never reported: single letters are almost never
/// misspellings, and they'd flag list markers, initials, and units.
const MIN_WORD_LEN: usize = 2;

/// Why spell checking couldn't start.
#[derive(Debug, Clone)]
pub enum SpellError {
    /// No `<language>.aff`/`.dic` pair turned up in any search location.
    NoDictionary {
        language: String,
        /// Where the user can put one, when the config directory is resolvable.
        dictionary_dir: Option<PathBuf>,
    },
    /// A dictionary was found but could not be read.
    Unreadable { path: PathBuf, error: String },
    /// A dictionary was read but isn't a valid Hunspell pair.
    Invalid { source: String, error: String },
}

impl fmt::Display for SpellError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpellError::NoDictionary {
                language,
                dictionary_dir,
            } => match dictionary_dir {
                Some(dir) => write!(
                    f,
                    "No {language} dictionary found — put {language}.aff and {language}.dic in {}",
                    dir.display()
                ),
                None => write!(f, "No {language} dictionary found"),
            },
            SpellError::Unreadable { path, error } => {
                write!(f, "Cannot read dictionary {}: {error}", path.display())
            }
            SpellError::Invalid { source, error } => {
                write!(f, "Invalid dictionary ({source}): {error}")
            }
        }
    }
}

/// One misspelled word in the document: the leaf that holds it plus the byte
/// range it occupies in that leaf's flattened plain text — the same coordinate
/// system as [`DocumentPosition`], so the editor can select and replace it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Misspelling {
    pub path: TreePath,
    pub start: usize,
    pub end: usize,
    pub word: String,
}

impl Misspelling {
    /// Document position of the word's first character.
    pub fn position(&self) -> DocumentPosition {
        DocumentPosition::at(self.path.clone(), self.start)
    }

    /// Document position just past the word's last character.
    pub fn end_position(&self) -> DocumentPosition {
        DocumentPosition::at(self.path.clone(), self.end)
    }
}

/// A one-line excerpt around a misspelling, with the word's byte range inside
/// the excerpt so the dialog can highlight it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContextWindow {
    pub text: String,
    pub word_start: usize,
    pub word_end: usize,
}

impl ContextWindow {
    /// The excerpt split into the part before the word, the word itself, and
    /// the part after it.
    pub fn parts(&self) -> (&str, &str, &str) {
        (
            &self.text[..self.word_start],
            &self.text[self.word_start..self.word_end],
            &self.text[self.word_end..],
        )
    }
}

/// A loaded dictionary plus the words the user has added or ignored.
pub struct SpellChecker {
    dict: Dictionary,
    language: String,
    /// Where the dictionary came from, for status messages.
    source: String,
    /// File that "Add to Dictionary" appends to; `None` when the config
    /// directory can't be resolved (added words then last for the session).
    personal_path: Option<PathBuf>,
    /// Lowercased words ignored for the rest of the session ("Ignore All").
    ignored: HashSet<String>,
}

impl SpellChecker {
    /// Load the dictionary for `config`'s language, then layer the user's
    /// personal word list on top.
    pub fn load(config: &Config) -> Result<Self, SpellError> {
        let language = normalize_language(&config.spell_language);
        let mut checker = match &config.spell_dictionary {
            Some(path) => Self::from_dic_path(path, &language)?,
            None => Self::discover(&language)?,
        };
        checker.personal_path = Config::personal_dictionary_path();
        checker.load_personal_words();
        Ok(checker)
    }

    /// Load an explicit `.dic` path, taking the `.aff` from beside it.
    pub fn from_dic_path(dic_path: &Path, language: &str) -> Result<Self, SpellError> {
        let aff_path = dic_path.with_extension("aff");
        let aff = read_dictionary_file(&aff_path)?;
        let dic = read_dictionary_file(dic_path)?;
        Self::from_str(&aff, &dic, language, &dic_path.display().to_string())
    }

    /// Build a checker from dictionary contents already in memory.
    pub fn from_str(
        aff: &str,
        dic: &str,
        language: &str,
        source: &str,
    ) -> Result<Self, SpellError> {
        let dict = Dictionary::new(aff, dic).map_err(|error| SpellError::Invalid {
            source: source.to_string(),
            error: error.to_string(),
        })?;
        Ok(Self {
            dict,
            language: language.to_string(),
            source: source.to_string(),
            personal_path: None,
            ignored: HashSet::new(),
        })
    }

    /// Search for `language`'s dictionary: the user's own directory first (so a
    /// hand-installed dictionary always wins), then the bundled copy, then
    /// `$DICPATH` and the system directories. Preferring the bundle over the
    /// system dictionaries keeps American English identical on every machine.
    fn discover(language: &str) -> Result<Self, SpellError> {
        if let Some(dir) = Config::dictionary_dir()
            && let Some(dic) = dictionary_pair_in(&dir, language)
        {
            return Self::from_dic_path(&dic, language);
        }

        #[cfg(feature = "bundled-dictionary")]
        if language == bundled::LANGUAGE {
            return Self::from_str(bundled::AFF, bundled::DIC, language, "bundled en_US");
        }

        for dir in system_dictionary_dirs() {
            if let Some(dic) = dictionary_pair_in(&dir, language) {
                return Self::from_dic_path(&dic, language);
            }
        }

        Err(SpellError::NoDictionary {
            language: language.to_string(),
            dictionary_dir: Config::dictionary_dir(),
        })
    }

    /// Add every word in the personal word list. Unreadable or malformed lines
    /// are skipped: a broken word list must not stop spell checking.
    fn load_personal_words(&mut self) {
        let Some(path) = self.personal_path.clone() else {
            return;
        };
        let Ok(contents) = fs::read_to_string(&path) else {
            return;
        };
        for line in contents.lines() {
            let word = line.trim();
            if word.is_empty() || word.starts_with('#') {
                continue;
            }
            let _ = self.dict.add(word);
        }
    }

    /// The language tag this checker was loaded for.
    pub fn language(&self) -> &str {
        &self.language
    }

    /// Where the dictionary came from, for status messages.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Teach the dictionary a word and append it to the personal word list.
    /// The word is always accepted in memory; the returned error only reports
    /// that persisting it failed.
    pub fn add_word(&mut self, word: &str) -> Result<(), String> {
        let word = normalize_apostrophes(word);
        let _ = self.dict.add(&word);
        let Some(path) = &self.personal_path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|err| err.to_string())?;
        writeln!(file, "{word}").map_err(|err| err.to_string())
    }

    /// Skip a word for the rest of the session, without touching the
    /// dictionary on disk ("Ignore All").
    pub fn ignore_word(&mut self, word: &str) {
        self.ignored.insert(word.to_lowercase());
    }

    /// Whether `word` should be reported as a misspelling.
    pub fn is_misspelled(&self, word: &str) -> bool {
        if word.chars().count() < MIN_WORD_LEN {
            return false;
        }
        if self.ignored.contains(&word.to_lowercase()) {
            return false;
        }
        // Acronyms are conventionally left alone: a dictionary can't hold every
        // FTML, HTML, or TUI a document mentions.
        if !word.chars().any(char::is_lowercase) {
            return false;
        }
        let word = normalize_apostrophes(word);
        if self.dict.check(&word) {
            return false;
        }
        // A possessive of a word the dictionary knows: only nouns carry the `'s`
        // suffix flag, so "Pure's" or "FTML's" would otherwise be flagged even
        // though their stem is fine.
        if let Some(stem) = possessive_stem(&word)
            && self.dict.check(stem)
        {
            return false;
        }
        // Only after the dictionary has had its say: a word with an interior
        // capital that isn't a known name (`camelCase`, `iPhone`) reads as an
        // identifier rather than a misspelling.
        !is_mixed_case(&word)
    }

    /// Corrections for a misspelled word, best first.
    pub fn suggest(&self, word: &str) -> Vec<String> {
        let mut out = Vec::new();
        self.dict.suggest(&normalize_apostrophes(word), &mut out);
        out.truncate(MAX_SUGGESTIONS);
        out
    }

    /// Every misspelling in `document`, in document order.
    ///
    /// Code blocks, inline code, and read-only tables are skipped — code isn't
    /// prose, and a table's text can't be edited — as are URLs, e-mail
    /// addresses, file names, and identifiers (see [`is_prose_chunk`]).
    pub fn check_document(&self, document: &Document) -> Vec<Misspelling> {
        let mut out = Vec::new();
        for leaf in tree_walk::enumerate_leaves(document) {
            if matches!(leaf.kind, ParaKind::CodeBlock | ParaKind::Table) {
                continue;
            }
            let text = tree_walk::leaf_plain_text(document, &leaf.path);
            if text.is_empty() {
                continue;
            }
            let code = code_ranges(tree_walk::leaf_spans(document, &leaf.path).unwrap_or(&[]));
            for (start, word) in words(&text) {
                if code.iter().any(|&(from, to)| start >= from && start < to) {
                    continue;
                }
                if self.is_misspelled(word) {
                    out.push(Misspelling {
                        path: leaf.path.clone(),
                        start,
                        end: start + word.len(),
                        word: word.to_string(),
                    });
                }
            }
        }
        out
    }
}

/// Normalize a language tag to the `en_US` shape dictionary files use.
fn normalize_language(language: &str) -> String {
    let language = language.trim().replace('-', "_");
    match language.split_once('_') {
        Some((lang, region)) => format!("{}_{}", lang.to_lowercase(), region.to_uppercase()),
        None => language.to_lowercase(),
    }
}

fn read_dictionary_file(path: &Path) -> Result<String, SpellError> {
    fs::read_to_string(path).map_err(|error| SpellError::Unreadable {
        path: path.to_path_buf(),
        error: error.to_string(),
    })
}

/// The `.dic` of a complete dictionary pair for `language` in `dir`, if any.
/// Both the flat (`dir/en_US.dic`) and per-language (`dir/en_US/en_US.dic`)
/// layouts are recognized, as is a hyphenated tag (`en-US`).
fn dictionary_pair_in(dir: &Path, language: &str) -> Option<PathBuf> {
    let hyphenated = language.replace('_', "-");
    let mut names = vec![language.to_string()];
    if hyphenated != language {
        names.push(hyphenated);
    }
    for name in names {
        for dic in [
            dir.join(format!("{name}.dic")),
            dir.join(&name).join(format!("{name}.dic")),
        ] {
            if dic.is_file() && dic.with_extension("aff").is_file() {
                return Some(dic);
            }
        }
    }
    None
}

/// Directories searched for dictionaries after the user's own and the bundled
/// one: `$DICPATH` (as Hunspell reads it) plus the conventional system
/// locations on Linux, macOS, and Homebrew installs.
fn system_dictionary_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = env::var_os("DICPATH")
        .map(|paths| env::split_paths(&paths).collect())
        .unwrap_or_default();
    dirs.extend(
        [
            "/usr/share/hunspell",
            "/usr/local/share/hunspell",
            "/opt/homebrew/share/hunspell",
            "/usr/share/myspell",
            "/usr/share/myspell/dicts",
            "/Library/Spelling",
            "/System/Library/Spelling",
        ]
        .into_iter()
        .map(PathBuf::from),
    );
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join("Library").join("Spelling"));
        dirs.push(home.join(".local").join("share").join("hunspell"));
    }
    dirs
}

/// Byte ranges of inline code within a leaf's flattened plain text.
///
/// The flattening is `span.text` followed by each child in turn (matching
/// `rutle`'s `leaf_plain_text`), so walking the spans in the same order tracks
/// the offsets exactly.
fn code_ranges(spans: &[Span]) -> Vec<(usize, usize)> {
    fn walk(spans: &[Span], offset: &mut usize, in_code: bool, out: &mut Vec<(usize, usize)>) {
        for span in spans {
            let in_code = in_code || span.style == InlineStyle::Code;
            let start = *offset;
            *offset += span.text.len();
            if in_code && *offset > start {
                out.push((start, *offset));
            }
            walk(&span.children, offset, in_code, out);
        }
    }

    let mut out = Vec::new();
    let mut offset = 0;
    walk(spans, &mut offset, false, &mut out);
    out
}

/// Every spell-checkable word in `text`, as `(byte offset, word)`.
fn words(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    for (chunk_start, chunk) in whitespace_chunks(text) {
        if !is_prose_chunk(chunk) {
            continue;
        }
        for (offset, word) in chunk_words(chunk) {
            out.push((chunk_start + offset, word));
        }
    }
    out
}

/// Split `text` on whitespace, keeping each chunk's byte offset.
fn whitespace_chunks(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start = None;
    for (index, ch) in text.char_indices() {
        if ch.is_whitespace() {
            if let Some(from) = start.take() {
                out.push((from, &text[from..index]));
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if let Some(from) = start {
        out.push((from, &text[from..]));
    }
    out
}

/// Whether a whitespace-delimited chunk is prose worth checking, as opposed to
/// something no dictionary can vouch for: a URL, an e-mail address, a path, a
/// file name, a version number, or a `snake_case` identifier.
fn is_prose_chunk(chunk: &str) -> bool {
    if chunk.contains("://") || chunk.contains('@') || chunk.contains('_') || chunk.contains('\\') {
        return false;
    }
    // `get` rather than a slice: a chunk can start with a multi-byte character,
    // and slicing four bytes into one of those would panic.
    if chunk
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("www."))
    {
        return false;
    }
    // A leading dot marks a file extension (".ftml", ".md").
    if let Some(rest) = chunk.strip_prefix('.')
        && rest.starts_with(|ch: char| ch.is_alphanumeric())
    {
        return false;
    }
    // A dot *between* two alphanumerics marks a domain, file name, or version
    // ("example.com", "app.rs", "1.2") — unlike the dot that ends a sentence.
    let chars: Vec<char> = chunk.chars().collect();
    !chars.windows(3).any(|window| {
        window[1] == '.' && window[0].is_alphanumeric() && window[2].is_alphanumeric()
    })
}

/// Words inside one chunk: runs of letters, with apostrophes allowed between
/// them ("don't", "we’ll") but never leading or trailing ("dogs'" → "dogs").
fn chunk_words(chunk: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let mut word_end = 0;
    for (index, ch) in chunk.char_indices() {
        if ch.is_alphabetic() {
            if start.is_none() {
                start = Some(index);
            }
            word_end = index + ch.len_utf8();
        } else if is_apostrophe(ch) && start.is_some() {
            // Keep the word open: it only stays part of it if a letter follows,
            // which `word_end` decides.
        } else if let Some(from) = start.take() {
            out.push((from, &chunk[from..word_end]));
        }
    }
    if let Some(from) = start {
        out.push((from, &chunk[from..word_end]));
    }
    out
}

fn is_apostrophe(ch: char) -> bool {
    ch == '\'' || ch == '\u{2019}'
}

/// The stem of an English possessive: `Pure's` → `Pure`. `None` when the word
/// isn't one.
fn possessive_stem(word: &str) -> Option<&str> {
    let stem = word
        .strip_suffix("'s")
        .or_else(|| word.strip_suffix("'S"))?;
    (!stem.is_empty()).then_some(stem)
}

/// Map typographic apostrophes onto the ASCII form the dictionaries use.
fn normalize_apostrophes(word: &str) -> String {
    word.replace('\u{2019}', "'")
}

/// Whether a word has an interior capital (`camelCase`, `iPhone`) — the shape
/// of an identifier rather than a misspelled word.
fn is_mixed_case(word: &str) -> bool {
    word.chars()
        .zip(word.chars().skip(1))
        .any(|(prev, next)| prev.is_lowercase() && next.is_uppercase())
}

/// A single-line excerpt of the paragraph holding `misspelling`, at most `width`
/// columns wide, for showing the word in context.
pub fn document_context(
    document: &Document,
    misspelling: &Misspelling,
    width: usize,
) -> ContextWindow {
    let text = tree_walk::leaf_plain_text(document, &misspelling.path);
    context_window(&text, misspelling.start, misspelling.end, width)
}

/// A single-line excerpt of `text` around the word at `start..end`, at most
/// `width` columns wide, with `…` marking either trimmed end.
pub fn context_window(text: &str, start: usize, end: usize, width: usize) -> ContextWindow {
    let start = start.min(text.len());
    let end = end.clamp(start, text.len());
    let word_chars = text[start..end].chars().count();
    // Room for the word itself plus context on both sides, never less than the
    // word and an ellipsis on each side.
    let budget = width.max(word_chars + 2).saturating_sub(word_chars + 2);
    let before_budget = budget / 2;
    let after_budget = budget - before_budget;

    let mut left = start;
    for _ in 0..before_budget {
        match text[..left].chars().next_back() {
            Some(ch) => left -= ch.len_utf8(),
            None => break,
        }
    }
    let mut right = end;
    for _ in 0..after_budget {
        match text[right..].chars().next() {
            Some(ch) => right += ch.len_utf8(),
            None => break,
        }
    }

    let mut window = String::new();
    if left > 0 {
        window.push('…');
    }
    window.push_str(&one_line(&text[left..start]));
    let word_start = window.len();
    window.push_str(&one_line(&text[start..end]));
    let word_end = window.len();
    window.push_str(&one_line(&text[end..right]));
    if right < text.len() {
        window.push('…');
    }
    ContextWindow {
        text: window,
        word_start,
        word_end,
    }
}

/// Flatten hard breaks and tabs to spaces so an excerpt stays on one row. Every
/// replacement is one byte for one byte, so offsets into the result still line
/// up with the source.
fn one_line(text: &str) -> String {
    text.chars()
        .map(|ch| {
            if ch.is_control() || ch == '\t' {
                ' '
            } else {
                ch
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "spell_tests.rs"]
mod spell_tests;
