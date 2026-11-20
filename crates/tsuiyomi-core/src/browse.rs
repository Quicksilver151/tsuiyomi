//! Browse-level matching helpers.

/// Normalize a title for duplicate detection.
///
/// Case, punctuation, and repeated whitespace are ignored so "Solo Leveling!" and
/// "solo  leveling" compare equal, matching Mihon's title-based duplicate lookup.
pub fn normalized_title(title: &str) -> String {
    let mut normalized = String::with_capacity(title.len());
    let mut pending_space = false;
    for character in title.chars().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() {
            if pending_space && !normalized.is_empty() {
                normalized.push(' ');
            }
            normalized.push(character);
            pending_space = false;
        } else {
            pending_space = true;
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_normalization_ignores_case_punctuation_and_spacing() {
        assert_eq!(normalized_title("Solo Leveling!"), "solo leveling");
        assert_eq!(normalized_title("  Solo   Leveling  "), "solo leveling");
        assert_eq!(normalized_title("Re:Zero"), "re zero");
        assert_ne!(normalized_title("Solo Leveling"), "solo leveling 2");
    }
}
