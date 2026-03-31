const UTF8_BOM_CHAR: char = '\u{FEFF}';

pub fn remove_verbatim_escape_sequences(s: &str) -> String {
    s.replace("\\r", " ")
        .replace("\\n", " ")
        .replace("\\t", " ")
}

pub fn strip_utf8_bom_str(s: &str) -> &str {
    s.strip_prefix(UTF8_BOM_CHAR).unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_strip_utf8_bom_str_with_bom() {
        let s = "\u{FEFF}Hello World";
        assert_eq!(strip_utf8_bom_str(s), "Hello World");
    }

    #[test]
    fn test_strip_utf8_bom_str_without_bom() {
        let s = "Hello World";
        assert_eq!(strip_utf8_bom_str(s), "Hello World");
    }

    #[test]
    fn test_strip_utf8_bom_str_empty() {
        let s = "";
        assert_eq!(strip_utf8_bom_str(s), "");
    }

    #[test]
    fn test_strip_utf8_bom_str_only_bom() {
        let s = "\u{FEFF}";
        assert_eq!(strip_utf8_bom_str(s), "");
    }

    #[test]
    fn test_bom_character_is_not_whitespace() {
        let s = "\u{FEFF}Hello";
        assert_ne!(s.trim(), "Hello");
        assert_eq!(strip_utf8_bom_str(s), "Hello");
    }

    #[test]
    fn test_remove_verbatim_escape_sequences_basic() {
        let input = "line1\\nline2\\rline3\\tline4";
        let output = remove_verbatim_escape_sequences(input);
        assert_eq!(output, "line1 line2 line3 line4");
    }

    #[test]
    fn test_remove_verbatim_escape_sequences_only_backslash_n() {
        let input = "hello\\nworld";
        let output = remove_verbatim_escape_sequences(input);
        assert_eq!(output, "hello world");
    }

    #[test]
    fn test_remove_verbatim_escape_sequences_no_escapes() {
        let input = "normal text without escapes";
        let output = remove_verbatim_escape_sequences(input);
        assert_eq!(output, input);
    }

    #[test]
    fn test_remove_verbatim_escape_sequences_actual_newline() {
        let input = "line1\nline2";
        let output = remove_verbatim_escape_sequences(input);
        assert_eq!(output, "line1\nline2");
    }

    #[test]
    fn test_remove_verbatim_escape_sequences_multiple() {
        let input = "a\\nb\\nc\\n";
        let output = remove_verbatim_escape_sequences(input);
        assert_eq!(output, "a b c ");
    }

    #[test]
    fn test_remove_verbatim_escape_sequences_options_c_sample() {
        let input = "Try `progname --help' for more information.\\n";
        let output = remove_verbatim_escape_sequences(input);
        assert_eq!(output, "Try `progname --help' for more information. ");
    }
}
