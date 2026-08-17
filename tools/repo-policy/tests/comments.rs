//! What counts as the code half of a line, for every check that strips comments before reading.

use repo_policy::actions::code_of;

/// A comment is prose about a pin; code is where a pin is requested. Losing code to a
/// mis-parsed quote takes a real version literal out of the scan that exists to find it.
#[test]
fn an_escaped_quote_does_not_end_the_string_it_is_inside() {
    // The `#` is inside the shell string, so nothing here is a comment.
    let line = r#"        run: echo "foo\" # 1.2.3""#;
    assert_eq!(code_of(line), line);
}

#[test]
fn a_trailing_comment_is_removed() {
    assert_eq!(code_of("        uses: owner/action@abc123 # v1.2.3"), "        uses: owner/action@abc123 ");
}

#[test]
fn a_hash_inside_a_quoted_value_is_not_a_comment() {
    let line = r#"        run: echo "colour #1.2.3""#;
    assert_eq!(code_of(line), line);
}

#[test]
fn a_hash_with_no_leading_space_is_not_a_comment() {
    let line = "        run: cargo build --target=x#y";
    assert_eq!(code_of(line), line);
}

#[test]
fn an_apostrophe_in_a_comment_still_lets_the_comment_be_stripped() {
    // Unbalanced quoting falls back to a quote-blind scan, so a lone apostrophe in prose
    // does not keep the rest of the comment in the code half.
    let stripped = code_of("        # it's pinned at 1.2.3");
    assert!(!stripped.contains("1.2.3"), "kept the comment: {stripped:?}");
}
