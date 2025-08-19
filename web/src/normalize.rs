// Reusable normalization logic for mapping typographic chars to ASCII equivalents
// Keep in sync with the client input handler.

pub fn normalize_char(c: char) -> char {
    match c {
        // Curly single quotes/apostrophes → '
        '\u{2018}' | '\u{2019}' | '\u{201B}' | '\u{2032}' | '\u{FF07}' => '\'',
        // Curly/directional/angle double quotes → "
        '\u{201C}' | '\u{201D}' | '\u{201F}' | '\u{2033}' | '\u{00AB}' | '\u{00BB}' | '\u{2039}' | '\u{203A}' | '\u{FF02}' => '"',
    // Dashes and minus variants → - (swung dash handled separately below)
    '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}' |
    '\u{2212}' | '\u{FE58}' | '\u{FE63}' | '\u{FF0D}' | '\u{2043}' |
    '\u{2E3A}' | /* two-em dash */ '\u{2E3B}' /* three-em dash */ => '-',
    // Swung dash → map to ASCII tilde so users can type '~'
    '\u{2053}' => '~',
        // Ellipsis → treat as a single '.' for typing equivalence
        '\u{2026}' => '.',
    // Unicode spaces and line breaks → normal space
    // ASCII whitespace: space, tab, newlines, vertical tab, form feed, carriage return
    '\u{0009}' /* TAB */ | '\u{000A}' /* LF */ | '\u{000B}' /* VT */ | '\u{000C}' /* FF */ | '\u{000D}' /* CR */ |
    // NEL, LS, PS
    '\u{0085}' | '\u{2028}' | '\u{2029}' |
    // Various Unicode spaces
        '\u{00A0}' | '\u{2007}' | '\u{202F}' | '\u{2000}' | '\u{2001}' | '\u{2002}' | '\u{2003}' | '\u{2004}' | '\u{2005}' | '\u{2006}' | '\u{2008}' | '\u{2009}' | '\u{200A}' | '\u{205F}' | '\u{3000}' => ' ',
        _ => c,
    }
}

pub fn is_skippable(c: char) -> bool {
    matches!(
        c,
        // Zero-width and word-joiners
    '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}'
    // Soft hyphen (conditionally invisible)
    | '\u{00AD}'
    )
}

/// Check if the ASCII-typed string could advance through the expected passage,
/// using normalize_char for comparison and skipping invisible codepoints.
pub fn matches_normalized(expected: &str, typed: &str) -> bool {
    let mut ei = 0usize;
    let mut ti = 0usize;
    let echars: Vec<char> = expected.chars().collect();
    let tchars: Vec<char> = typed.chars().collect();
    while ei < echars.len() {
    let ec = echars[ei];
        if is_skippable(ec) { ei += 1; continue; }
        let en = normalize_char(ec);
        // No more typed chars left => failure
        if ti >= tchars.len() { return false; }
        let tn = normalize_char(tchars[ti]);
        if en == tn {
            ei += 1; ti += 1; continue;
        } else {
            return false;
        }
    }
    // All expected consumed; typed may have extra -> consider success only if typed fully used as well
    ti == tchars.len()
}

// Provide a comprehensive test passage string for UI testing
pub fn tests_passage() -> String {
    "\
\u{201C}You get in,\u{201D} he added, motioning to me with his tomahawk. What\u{2019}s all to myself\u{2014}the man\u{2019}s a human being just as I am.\n\
\u{2014} \u{2013},  \u{2012}, \u{2014}, \u{2015},  \u{2212},  \u{FF0D}.\n\
 \"double\" and 'single'. \u{2026}\n\
Hello\u{00A0}, \u{2009}, \u{200A}. \u{200B}, \u{00AD}.\n\
\u{2E3A}, \u{2E3B}, \u{2053}.\n\
End.".to_string()
}

#[cfg(test)]
mod tests {
    use super::{normalize_char as n, is_skippable, matches_normalized};

    fn eq(a: char, b: char) -> bool { n(a) == n(b) }

    #[test]
    fn quotes_normalize() {
        assert!(eq('\'', '\u{2019}')); // apostrophe
        assert!(eq('"', '\u{201C}')); // left double quote
        assert!(eq('"', '\u{201D}')); // right double quote
        assert!(eq('"', '\u{00AB}')); // «
        assert!(eq('"', '\u{00BB}')); // »
    }

    #[test]
    fn dashes_normalize() {
        // hyphen to en dash/em dash/minus
    for c in ['\u{2010}','\u{2011}','\u{2012}','\u{2013}','\u{2014}','\u{2015}','\u{2212}','\u{FF0D}','\u{2E3A}','\u{2E3B}'] { assert!(eq('-', c)); }
    // Swung dash should match tilde
    assert!(eq('~', '\u{2053}'));
    }

    #[test]
    fn spaces_normalize() {
        for c in ['\u{00A0}','\u{2007}','\u{202F}','\u{2000}','\u{2001}','\u{2002}','\u{2003}','\u{2004}','\u{2005}','\u{2006}','\u{2008}','\u{2009}','\u{200A}','\u{205F}','\u{3000}'] { assert!(eq(' ', c)); }
    }

    #[test]
    fn ellipsis_normalize() { assert_eq!(n('\u{2026}'), '.'); }

    #[test]
    fn linebreaks_normalize() {
        // Map common line breaks and tabs to space for typing equivalence
        for c in ['\u{0009}', '\u{000A}', '\u{000B}', '\u{000C}', '\u{000D}', '\u{0085}', '\u{2028}', '\u{2029}'] { assert!(eq(' ', c)); }
    }

    #[test]
    fn skippables() {
        assert!(is_skippable('\u{200B}')); // zero-width space
        assert!(is_skippable('\u{00AD}')); // soft hyphen
        assert!(!is_skippable('\u{2009}')); // thin space should not be auto-skipped
        assert!(!is_skippable('\u{00A0}')); // nbsp should not be auto-skipped
        assert!(!is_skippable(' ')); // normal space should not be skippable
    }

    #[test]
    fn passage_quotes_match_ascii() {
        let expected = "\u{201C}You gettee in,\u{201D}"; // “You gettee in,”
        let typed = "\"You gettee in,\"";              // "You gettee in,"
        assert!(matches_normalized(expected, typed));
        let expected2 = "\u{2018}it\u{2019}s fine\u{2019}"; // ‘it’s fine’
        let typed2 = "'it's fine'";                           // 'it's fine'
        assert!(matches_normalized(expected2, typed2));
    }

    #[test]
    fn passage_dashes_match_ascii() {
        // “added:—.” should accept ":-."
        let expected = "added:\u{2014}.";
        let typed = "added:-.";
        assert!(matches_normalized(expected, typed));
        // Two-em dash
        let expected2 = "wait\u{2E3A}go";
        let typed2 = "wait-go";
        assert!(matches_normalized(expected2, typed2));
    // Swung dash should accept tilde
    let expected3 = "swing\u{2053}dash";
    let typed3 = "swing~dash";
    assert!(matches_normalized(expected3, typed3));
    }
}
