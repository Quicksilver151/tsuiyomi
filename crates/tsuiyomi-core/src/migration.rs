//! Title matching for source migration.
//!
//! The matching rules follow Mihon's `BaseSmartSearchEngine`: compare normalized
//! titles and accept candidates above a similarity threshold, with an optional
//! deep-search query set that strips punctuation and bracket noise.

use crate::browse::normalized_title;

pub const MATCH_THRESHOLD: f64 = 0.5;

/// Normalized Levenshtein similarity in `0.0..=1.0`.
pub fn title_similarity(left: &str, right: &str) -> f64 {
    let left = normalized_title(left);
    let right = normalized_title(right);
    if left == right {
        return 1.0;
    }
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let max = left.chars().count().max(right.chars().count()) as f64;
    let distance = levenshtein(&left, &right) as f64;
    (1.0 - distance / max).clamp(0.0, 1.0)
}

/// Pick the highest-scoring candidate above the threshold.
pub fn best_match(
    title: &str,
    candidates: impl IntoIterator<Item = (usize, String)>,
) -> Option<(usize, f64)> {
    candidates
        .into_iter()
        .map(|(index, candidate)| (index, title_similarity(title, &candidate)))
        .filter(|(_, score)| *score >= MATCH_THRESHOLD)
        .max_by(|left, right| {
            left.1
                .partial_cmp(&right.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

/// Deep-search query variants: full cleaned title, two largest words, largest
/// word, first two words, first word.
pub fn deep_queries(title: &str) -> Vec<String> {
    let cleaned = clean_title(title);
    let words: Vec<&str> = cleaned.split_whitespace().collect();
    if words.is_empty() {
        return vec![title.trim().to_owned()];
    }
    let mut largest = words.clone();
    largest.sort_by_key(|word| std::cmp::Reverse(word.len()));
    let mut queries = vec![
        cleaned.clone(),
        largest
            .iter()
            .take(2)
            .copied()
            .collect::<Vec<_>>()
            .join(" "),
        largest.first().copied().unwrap_or_default().to_owned(),
        words.iter().take(2).copied().collect::<Vec<_>>().join(" "),
        words.first().copied().unwrap_or_default().to_owned(),
    ];
    queries.retain(|query| !query.trim().is_empty());
    queries.dedup();
    queries
}

fn clean_title(title: &str) -> String {
    let lower = title.to_lowercase();
    let mut cleaned = String::with_capacity(lower.len());
    let mut depth = 0usize;
    for character in lower.chars() {
        match character {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ if depth == 0 => cleaned.push(character),
            _ => {}
        }
    }
    cleaned
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || character == '-' || character == ' ' {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn levenshtein(left: &str, right: &str) -> usize {
    let right: Vec<char> = right.chars().collect();
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0usize; right.len() + 1];
    for (row, left_char) in left.chars().enumerate() {
        current[0] = row + 1;
        for (column, right_char) in right.iter().enumerate() {
            let cost = usize::from(left_char != *right_char);
            current[column + 1] = (previous[column + 1] + 1)
                .min(current[column] + 1)
                .min(previous[column] + cost);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn similarity_ignores_case_punctuation_and_brackets() {
        assert_eq!(title_similarity("Solo Leveling!", "solo leveling"), 1.0);
        assert!(title_similarity("Solo Leveling", "Solo Leveling 2") > 0.7);
        assert!(title_similarity("Berserk", "Naruto") < MATCH_THRESHOLD);
    }

    #[test]
    fn deep_queries_use_cleaned_words() {
        let queries = deep_queries("Solo Leveling [Official]");
        assert_eq!(queries[0], "solo leveling");
        assert!(queries.iter().any(|query| query == "leveling"));
    }

    #[test]
    fn best_match_picks_the_highest_score() {
        let candidates = vec![
            (0, "Berserk".to_owned()),
            (1, "Berserk of Gluttony".to_owned()),
            (2, "Naruto".to_owned()),
        ];
        assert_eq!(
            best_match("Berserk", candidates).map(|(index, _)| index),
            Some(0)
        );
    }
}
