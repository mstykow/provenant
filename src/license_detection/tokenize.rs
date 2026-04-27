// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! Text tokenization and normalization.
//!
//! Tokenization converts text into a sequence of tokens that can be matched
//! against license rules. This module implements ScanCode-compatible tokenization.

use crate::license_detection::index::dictionary::{QueryToken, TokenDictionary};
use regex::Regex;
use std::collections::HashSet;
use std::ops::Range;
use std::sync::LazyLock;

const REQUIRED_PHRASE_OPEN: &str = "{{";
const REQUIRED_PHRASE_CLOSE: &str = "}}";

/// Common words that are ignored from matching such as HTML tags, XML entities, etc.
///
/// This is the Rust equivalent of the Python STOPWORDS frozenset from
/// reference/scancode-toolkit/src/licensedcode/stopwords.py
pub(crate) static STOPWORDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut set = HashSet::new();

    // common XML character references as &quot;
    for &word in &["amp", "apos", "gt", "lt", "nbsp", "quot"] {
        set.insert(word);
    }

    // common html tags as <a href=https://link ...> dfsdfsdf</a>
    for &word in &[
        "a",
        "abbr",
        "alt",
        "blockquote",
        "body",
        "br",
        "class",
        "div",
        "em",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "hr",
        "href",
        "img",
        "li",
        "ol",
        "p",
        "pre",
        "rel",
        "script",
        "span",
        "src",
        "td",
        "th",
        "tr",
        "ul",
    ] {
        set.insert(word);
    }

    // comment line markers
    set.insert("rem"); // batch files
    set.insert("dnl"); // autotools

    // doc book tags as <para>
    set.insert("para");
    set.insert("ulink");

    // Some HTML punctuations and entities all as &emdash;
    for &word in &[
        "bdquo", "bull", "bullet", "colon", "comma", "emdash", "emsp", "ensp", "ge", "hairsp",
        "ldquo", "ldquor", "le", "lpar", "lsaquo", "lsquo", "lsquor", "mdash", "ndash", "numsp",
        "period", "puncsp", "raquo", "rdquo", "rdquor", "rpar", "rsaquo", "rsquo", "rsquor",
        "sbquo", "semi", "thinsp", "tilde",
    ] {
        set.insert(word);
    }

    // some xml char entities
    set.insert("x3c");
    set.insert("x3e");

    // seen in many CSS
    for &word in &[
        "lists", "side", "nav", "height", "auto", "border", "padding", "width",
    ] {
        set.insert(word);
    }

    // seen in Perl PODs
    set.insert("head1");
    set.insert("head2");
    set.insert("head3");

    // common in C literals
    set.insert("printf");

    // common in shell
    set.insert("echo");

    set
});

/// Splits on whitespace and punctuation: keep only characters and numbers and + when in the middle or end of a word.
///
/// The pattern is equivalent to Python's: `[^_\W]+\+?[^_\W]*`
/// - `[^_\W]+` - one or more characters that are NOT underscore and NOT non-word (i.e., alphanumeric including Unicode)
/// - `\+?` - optional plus sign (important for license names like "GPL2+")
/// - `[^_\W]*` - zero or more alphanumeric characters (including Unicode)
///
/// This matches word-like sequences while preserving trailing `+` characters.
/// Uses Unicode-aware matching to match Python's `re.UNICODE` behavior.
static QUERY_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[^_\W]+\+?[^_\W]*").expect("Invalid regex pattern"));

/// Tokenizes text to match index rules and queries.
///
/// Splits text into tokens using regex pattern, normalizes each token (lowercase),
/// and filters out empty strings and stopwords.
///
/// # Returns
/// A vector of token strings.
pub fn tokenize(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }

    let mut tokens = Vec::new();
    let lowercase_text = text.to_lowercase();

    for cap in QUERY_PATTERN.find_iter(&lowercase_text) {
        let token = cap.as_str();

        // Filter out empty strings and stopwords
        if !token.is_empty() && !STOPWORDS.contains(token) {
            tokens.push(token.to_string());
        }
    }

    tokens
}

/// Tokenizes text without filtering stopwords.
///
/// This is used for query text where stopwords are handled at a later stage.
///
/// # Returns
/// A vector of token strings.
pub fn tokenize_without_stopwords(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }

    let mut tokens = Vec::new();
    let lowercase_text = text.to_lowercase();

    for cap in QUERY_PATTERN.find_iter(&lowercase_text) {
        let token = cap.as_str();

        // Filter out empty strings but keep stopwords
        if !token.is_empty() {
            tokens.push(token.to_string());
        }
    }

    tokens
}

/// Tokenizes text and returns QueryTokens directly, avoiding string allocation.
///
/// This is the primary tokenization function for query processing.
/// Tokens are classified against the dictionary immediately:
/// - Known tokens → QueryToken::Known(KnownToken)
/// - Unknown tokens → QueryToken::Unknown
/// - Stopwords → QueryToken::Stopword
///
/// # Returns
/// A vector of QueryTokens (no string allocation).
pub fn tokenize_as_ids(text: &str, dictionary: &TokenDictionary) -> Vec<QueryToken> {
    if text.is_empty() {
        return Vec::new();
    }

    let mut tokens = Vec::new();
    let stopwords_set = &*STOPWORDS;

    let lowercase_text = text.to_lowercase();

    for cap in QUERY_PATTERN.find_iter(&lowercase_text) {
        let token = cap.as_str();
        if token.is_empty() {
            continue;
        }

        if stopwords_set.contains(token) {
            tokens.push(QueryToken::Stopword);
        } else {
            tokens.push(dictionary.classify_query_token(token));
        }
    }

    tokens
}

/// Count tokens in text without allocating strings.
///
/// Used for line length detection without memory overhead.
pub fn count_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }

    let lowercase_text = text.to_lowercase();
    let stopwords_set = &*STOPWORDS;

    QUERY_PATTERN
        .find_iter(&lowercase_text)
        .filter(|m| !m.as_str().is_empty() && !stopwords_set.contains(m.as_str()))
        .count()
}

/// Parse {{...}} required phrase markers from rule text.
///
/// Returns list of token position ranges for required phrases.
/// The spans represent the positions (after tokenization) of tokens
/// that MUST be matched for the rule to be considered valid.
///
/// # Arguments
/// * `text` - The rule text containing optional {{...}} markers
///
/// # Returns
/// A vector of Range<usize> representing token positions for each required phrase.
/// Empty vector if no valid required phrases found.
///
/// Example: `This is {{enclosed}} in braces` yields `vec![2..3]`.
///
/// Based on Python: `get_existing_required_phrase_spans()` in tokenize.py:122-174
pub fn parse_required_phrase_spans(text: &str) -> Vec<Range<usize>> {
    let mut spans = Vec::new();
    let mut in_required_phrase = false;
    let mut current_phrase_positions: Vec<usize> = Vec::new();
    let mut ipos = 0usize;

    for token in required_phrase_tokenizer(text) {
        if token == REQUIRED_PHRASE_OPEN {
            if in_required_phrase {
                log::warn!(
                    "Invalid rule with nested required phrase {{ {{ braces: {}",
                    text
                );
                return Vec::new();
            }
            in_required_phrase = true;
        } else if token == REQUIRED_PHRASE_CLOSE {
            if in_required_phrase {
                if !current_phrase_positions.is_empty() {
                    let min_pos = *current_phrase_positions.iter().min().unwrap_or(&0);
                    let max_pos = *current_phrase_positions.iter().max().unwrap_or(&0);
                    spans.push(min_pos..max_pos + 1);
                    current_phrase_positions.clear();
                } else {
                    log::warn!(
                        "Invalid rule with empty required phrase {{}} braces: {}",
                        text
                    );
                    return Vec::new();
                }
                in_required_phrase = false;
            } else {
                log::warn!(
                    "Invalid rule with dangling required phrase missing closing braces: {}",
                    text
                );
                return Vec::new();
            }
        } else {
            if in_required_phrase {
                current_phrase_positions.push(ipos);
            }
            ipos += 1;
        }
    }

    if !current_phrase_positions.is_empty() || in_required_phrase {
        log::warn!(
            "Invalid rule with dangling required phrase missing final closing braces: {}",
            text
        );
        return Vec::new();
    }

    spans
}

/// Tokenizer for parsing required phrase markers.
///
/// Yields tokens including "{{" and "}}" markers.
/// Similar to the required_phrase_tokenizer generator in Python.
fn required_phrase_tokenizer(text: &str) -> RequiredPhraseTokenIter {
    let lowercase_text = text.to_lowercase();
    let tokens: Vec<TokenKind> = REQUIRED_PHRASE_PATTERN
        .find_iter(&lowercase_text)
        .filter_map(|m| {
            let token = m.as_str();
            if token == REQUIRED_PHRASE_OPEN {
                Some(TokenKind::Open)
            } else if token == REQUIRED_PHRASE_CLOSE {
                Some(TokenKind::Close)
            } else if !token.is_empty() && !STOPWORDS.contains(token) {
                Some(TokenKind::Word)
            } else {
                None
            }
        })
        .collect();
    RequiredPhraseTokenIter { tokens, pos: 0 }
}

#[derive(Clone, Copy, PartialEq)]
enum TokenKind {
    Open,
    Close,
    Word,
}

struct RequiredPhraseTokenIter {
    tokens: Vec<TokenKind>,
    pos: usize,
}

impl Iterator for RequiredPhraseTokenIter {
    type Item = &'static str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.tokens.len() {
            return None;
        }
        let token = self.tokens[self.pos];
        self.pos += 1;
        Some(match token {
            TokenKind::Open => REQUIRED_PHRASE_OPEN,
            TokenKind::Close => REQUIRED_PHRASE_CLOSE,
            TokenKind::Word => "word",
        })
    }
}

/// Pattern for matching words and braces in required phrase tokenizer.
/// Equivalent to Python's: `(?:[^_\W]+\+?[^_\W]*|\{\{|\}\})`
static REQUIRED_PHRASE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:[^_\W]+\+?[^_\W]*|\{\{|\}\})").expect("Invalid required phrase pattern")
});

/// Tokenize text and track stopwords by position.
///
/// Returns (tokens, stopwords_by_pos) where:
/// - tokens: vector of token strings
/// - stopwords_by_pos: mapping from token position to count of stopwords after that position
///
/// Based on Python: `index_tokenizer_with_stopwords()` in tokenize.py:247-306
pub fn tokenize_with_stopwords(
    text: &str,
) -> (Vec<String>, std::collections::HashMap<Option<usize>, usize>) {
    if text.is_empty() {
        return (Vec::new(), std::collections::HashMap::new());
    }

    let mut tokens = Vec::new();
    let mut stopwords_by_pos = std::collections::HashMap::new();

    let mut pos: Option<usize> = None;
    let lowercase_text = text.to_lowercase();

    for cap in QUERY_PATTERN.find_iter(&lowercase_text) {
        let token = cap.as_str();
        if token.is_empty() {
            continue;
        }

        if STOPWORDS.contains(token) {
            *stopwords_by_pos.entry(pos).or_insert(0) += 1;
        } else {
            pos = Some(pos.map_or(0, |p| p + 1));
            tokens.push(token.to_string());
        }
    }

    (tokens, stopwords_by_pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_empty() {
        let result = tokenize("");
        assert!(result.is_empty());
    }

    #[test]
    fn test_tokenize_simple() {
        let result = tokenize("Hello World");
        assert_eq!(result, vec!["hello", "world"]);
    }

    #[test]
    fn test_tokenize_with_punctuation() {
        let result = tokenize("Hello, World! This is a test.");
        // Note: 'a' is filtered because it's in STOPWORDS (it's an HTML tag)
        assert_eq!(result, vec!["hello", "world", "this", "is", "test"]);
    }

    #[test]
    fn test_tokenize_with_spaces() {
        let result = tokenize("some Text with   spAces!");
        assert_eq!(result, vec!["some", "text", "with", "spaces"]);
    }

    #[test]
    fn test_tokenize_with_plus() {
        let result = tokenize("GPL2+ and GPL3");
        assert_eq!(result, vec!["gpl2+", "and", "gpl3"]);
    }

    #[test]
    fn test_tokenize_filters_stopwords() {
        let result = tokenize("Hello div World p");
        assert_eq!(result, vec!["hello", "world"]);
    }

    #[test]
    fn test_tokenize_with_special_chars() {
        let result = tokenize("special+-_!@ chars");
        // Based on Python: ['special+', 'chars']
        assert_eq!(result, vec!["special+", "chars"]);
    }

    #[test]
    fn test_tokenize_with_underscores() {
        let result = tokenize("hello_world foo_bar");
        assert_eq!(result, vec!["hello", "world", "foo", "bar"]);
    }

    #[test]
    fn test_tokenize_with_numbers() {
        let result = tokenize("version 2.0 and 3.0");
        assert_eq!(result, vec!["version", "2", "0", "and", "3", "0"]);
    }

    #[test]
    fn test_tokenize_without_stopwords_keeps_html_tags() {
        let result = tokenize_without_stopwords("Hello div World p");
        assert_eq!(result, vec!["hello", "div", "world", "p"]);
    }

    #[test]
    fn test_tokenize_without_stopwords_empty() {
        let result = tokenize_without_stopwords("");
        assert!(result.is_empty());
    }

    #[test]
    fn test_tokenization_with_plus_in_middle() {
        let result = tokenize("C++ and GPL+");
        assert_eq!(result, vec!["c+", "and", "gpl+"]);
    }

    #[test]
    fn test_tokenization_braces() {
        let result = tokenize("{{Hi}}some {{}}Text with{{noth+-_!@ing}}   {{junk}}spAces!");
        assert_eq!(
            result,
            vec![
                "hi", "some", "text", "with", "noth+", "ing", "junk", "spaces"
            ]
        );
    }

    #[test]
    fn test_tokenize_with_ampersand() {
        let result = tokenize("some &quot< markup &gt\"");
        assert_eq!(result, vec!["some", "markup"]);
    }

    #[test]
    #[allow(non_snake_case)]
    fn test_query_tokenizer_brace_case() {
        let result = tokenize("{{}some }}Text with   spAces! + _ -");
        assert_eq!(result, vec!["some", "text", "with", "spaces"]);
    }

    #[test]
    fn test_tokenize_unicode_characters() {
        // With Unicode pattern [^_\W], we match Unicode letters like Python's re.UNICODE
        let result = tokenize("hello 世界 mir");
        assert_eq!(result, vec!["hello", "世界", "mir"]);
    }

    #[test]
    fn test_tokenize_only_special_chars() {
        let result = tokenize("!@#$%^&*()");
        assert!(result.is_empty());
    }

    #[test]
    fn test_tokenize_only_punctuation() {
        let result = tokenize(".,;:!?-_=+[]{}()");
        assert!(result.is_empty());
    }

    #[test]
    fn test_tokenize_only_stopwords() {
        let result = tokenize("div p a br");
        assert!(result.is_empty());
    }

    #[test]
    fn test_tokenize_mixed_stopwords_and_words() {
        let result = tokenize("div hello p world a test");
        assert_eq!(result, vec!["hello", "world", "test"]);
    }

    #[test]
    fn test_tokenize_very_long_text() {
        let words: Vec<String> = (0..1000).map(|i| format!("word{}", i)).collect();
        let text = words.join(" ");
        let result = tokenize(&text);
        assert_eq!(result.len(), 1000);
        assert_eq!(result[0], "word0");
        assert_eq!(result[999], "word999");
    }

    #[test]
    fn test_tokenize_with_newlines_and_tabs() {
        let result = tokenize("hello\nworld\ttest");
        assert_eq!(result, vec!["hello", "world", "test"]);
    }

    #[test]
    fn test_tokenize_with_carriage_return() {
        let result = tokenize("hello\r\nworld\rtest");
        assert_eq!(result, vec!["hello", "world", "test"]);
    }

    #[test]
    fn test_tokenize_trailing_plus() {
        let result = tokenize("GPL2+ LGPL3+");
        assert_eq!(result, vec!["gpl2+", "lgpl3+"]);
    }

    #[test]
    fn test_tokenize_leading_plus() {
        let result = tokenize("+hello +world");
        assert_eq!(result, vec!["hello", "world"]);
    }

    #[test]
    fn test_tokenize_without_stopwords_preserves_all() {
        let result = tokenize_without_stopwords("div p a br");
        assert_eq!(result, vec!["div", "p", "a", "br"]);
    }

    #[test]
    fn test_tokenize_without_stopwords_unicode() {
        // With Unicode pattern [^_\W], we match Unicode letters like Python's re.UNICODE
        let result = tokenize_without_stopwords("hello 世界");
        assert_eq!(result, vec!["hello", "世界"]);
    }

    #[test]
    fn test_tokenize_without_stopwords_only_special() {
        let result = tokenize_without_stopwords("!@#$%");
        assert!(result.is_empty());
    }

    #[test]
    fn test_tokenize_consecutive_plus() {
        let result = tokenize("a++b");
        assert_eq!(result, vec!["a+", "b"]);
    }

    #[test]
    fn test_tokenize_hyphenated_words() {
        let result = tokenize("some-thing foo-bar");
        assert_eq!(result, vec!["some", "thing", "foo", "bar"]);
    }

    #[test]
    fn test_tokenize_email_address() {
        let result = tokenize("test@example.com");
        assert_eq!(result, vec!["test", "example", "com"]);
    }

    #[test]
    fn test_tokenize_url() {
        let result = tokenize("https://example.com/path");
        assert_eq!(result, vec!["https", "example", "com", "path"]);
    }

    #[test]
    fn test_tokenize_version_number() {
        let result = tokenize("version 1.2.3");
        assert_eq!(result, vec!["version", "1", "2", "3"]);
    }

    #[test]
    fn test_tokenize_xml_entities() {
        let result = tokenize("&lt;div&gt;hello&lt;/div&gt;");
        assert_eq!(result, vec!["hello"]);
    }

    #[test]
    fn test_tokenize_whitespace_only() {
        let result = tokenize("   \t\n\r   ");
        assert!(result.is_empty());
    }

    #[test]
    fn test_tokenize_single_char() {
        let result = tokenize("a");
        assert!(result.is_empty());
    }

    #[test]
    fn test_tokenize_single_word() {
        let result = tokenize("hello");
        assert_eq!(result, vec!["hello"]);
    }

    #[test]
    fn test_tokenize_numbers_only() {
        let result = tokenize("123 456 789");
        assert_eq!(result, vec!["123", "456", "789"]);
    }

    #[test]
    fn test_tokenize_alphanumeric_mixed() {
        let result = tokenize("abc123 def456");
        assert_eq!(result, vec!["abc123", "def456"]);
    }

    #[test]
    fn test_tokenize_underscore_separated() {
        let result = tokenize("hello_world foo_bar_baz");
        assert_eq!(result, vec!["hello", "world", "foo", "bar", "baz"]);
    }

    #[test]
    fn test_tokenize_all_stopwords_from_list() {
        let result = tokenize("amp lt gt nbsp quot");
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_required_phrase_spans_single() {
        let text = "This is {{enclosed}} in braces";
        let spans = parse_required_phrase_spans(text);
        assert_eq!(spans, vec![2..3]);
    }

    #[test]
    fn test_parse_required_phrase_spans_multiword() {
        let text = "This is {{a required phrase}} here";
        let spans = parse_required_phrase_spans(text);
        assert_eq!(spans, vec![2..4]);
    }

    #[test]
    fn test_parse_required_phrase_spans_multiple() {
        let text = "{{First}} and {{second}} phrase";
        let spans = parse_required_phrase_spans(text);
        assert_eq!(spans, vec![0..1, 2..3]);
    }

    #[test]
    fn test_parse_required_phrase_spans_none() {
        let text = "No required phrases here";
        let spans = parse_required_phrase_spans(text);
        assert!(spans.is_empty());
    }

    #[test]
    fn test_parse_required_phrase_spans_empty_braces() {
        let text = "Empty {{}} braces";
        let spans = parse_required_phrase_spans(text);
        assert!(spans.is_empty());
    }

    #[test]
    fn test_parse_required_phrase_spans_nested() {
        let text = "Nested {{ outer {{ inner }} }} braces";
        let spans = parse_required_phrase_spans(text);
        assert!(spans.is_empty());
    }

    #[test]
    fn test_parse_required_phrase_spans_unclosed() {
        let text = "Unclosed {{ phrase here";
        let spans = parse_required_phrase_spans(text);
        assert!(spans.is_empty());
    }

    #[test]
    fn test_parse_required_phrase_spans_unopened() {
        let text = "Unopened }} phrase here";
        let spans = parse_required_phrase_spans(text);
        assert!(spans.is_empty());
    }

    #[test]
    fn test_tokenize_with_stopwords_basic() {
        let text = "hello div world p test";
        let (tokens, stopwords) = tokenize_with_stopwords(text);
        assert_eq!(tokens, vec!["hello", "world", "test"]);
        // "div" is stopword after "hello" (pos 0), "p" is stopword after "world" (pos 1)
        assert_eq!(stopwords.get(&Some(0)), Some(&1));
        assert_eq!(stopwords.get(&Some(1)), Some(&1));
    }

    #[test]
    fn test_tokenize_with_stopwords_empty() {
        let (tokens, stopwords) = tokenize_with_stopwords("");
        assert!(tokens.is_empty());
        assert!(stopwords.is_empty());
    }

    #[test]
    fn test_tokenize_with_stopwords_no_stopwords() {
        let text = "hello world test";
        let (tokens, stopwords) = tokenize_with_stopwords(text);
        assert_eq!(tokens, vec!["hello", "world", "test"]);
        assert!(stopwords.is_empty());
    }

    #[test]
    fn test_parse_required_phrase_spans_filters_stopwords_inside() {
        let text = "{{hello a world}}";
        let spans = parse_required_phrase_spans(text);
        assert_eq!(spans, vec![0..2]);
    }

    #[test]
    fn test_parse_required_phrase_spans_filters_stopwords_outside() {
        let text = "{{Hello}} a {{world}}";
        let spans = parse_required_phrase_spans(text);
        assert_eq!(spans, vec![0..1, 1..2]);
    }

    #[test]
    fn test_parse_required_phrase_spans_multiple_stopwords() {
        let text = "{{a p div hello}}";
        let spans = parse_required_phrase_spans(text);
        assert_eq!(spans, vec![0..1]);
    }

    #[test]
    fn test_parse_required_phrase_spans_case_insensitive_stopwords() {
        let text = "{{HELLO A WORLD}}";
        let spans = parse_required_phrase_spans(text);
        assert_eq!(spans, vec![0..2]);
    }
}
