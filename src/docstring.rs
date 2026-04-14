/// Count words in the prose portion of a docstring, ignoring `@tag` lines.
///
/// Strips block-comment delimiters (`/**`, `*/`, leading `*`), removes lines
/// that begin with a Javadoc/JSDoc/rustdoc tag (`@param`, `@return`, …),
/// and counts whitespace-separated words in whatever remains.
pub fn count_prose_words(text: &str) -> usize {
    text.lines()
        .map(|line| {
            // Strip block-comment delimiters and leading decoration
            let line = line.trim();
            let line = line.trim_start_matches("/**").trim();
            let line = line.trim_start_matches("*/").trim();
            let line = line.trim_start_matches('*').trim();
            // Strip Rust/Go/C# doc-comment prefixes
            let line = if line.starts_with("///") {
                line.trim_start_matches("///").trim()
            } else if line.starts_with("//") {
                line.trim_start_matches("//").trim()
            } else {
                line
            };
            line
        })
        .filter(|line| !line.starts_with('@'))
        .map(|line| line.split_whitespace().count())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_string_is_zero() {
        assert_eq!(count_prose_words(""), 0);
    }

    #[test]
    fn trivial_one_liner_counts_correctly() {
        // Raw extracted text after stripping /** and */
        assert_eq!(count_prose_words("Gets the user."), 3);
    }

    #[test]
    fn tag_only_line_is_zero() {
        assert_eq!(count_prose_words("@param id the user id"), 0);
    }

    #[test]
    fn prose_with_tags_counts_only_prose() {
        let text = "Returns the user.\n@param id the id\n@return Optional<User>";
        assert_eq!(count_prose_words(text), 3);
    }

    #[test]
    fn multiline_prose_accumulates() {
        let text = "Validates the email address format.\nThrows if null or empty.";
        assert_eq!(count_prose_words(text), 10);
    }

    #[test]
    fn block_comment_delimiters_are_stripped() {
        // As it comes out of the raw block comment
        let text = "/**\n * Gets the user.\n * @param id the id\n */";
        assert_eq!(count_prose_words(text), 3);
    }

    #[test]
    fn rust_triple_slash_prefix_stripped() {
        let text = "/// Validates input.\n/// Throws if null.";
        assert_eq!(count_prose_words(text), 5);
    }

    #[test]
    fn whitespace_only_is_zero() {
        assert_eq!(count_prose_words("   \n  \t  "), 0);
    }
}
