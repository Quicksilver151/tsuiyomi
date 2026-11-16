use std::cmp::Ordering;

/// Natural, case-insensitive comparison used for series, chapters, and pages.
pub fn natural_cmp(left: &str, right: &str) -> Ordering {
    natord::compare(&left.to_lowercase(), &right.to_lowercase())
}

/// Extract a chapter number from a directory or archive name.
///
/// This is a pragmatic subset of Mihon's `ChapterRecognition`: prefer numbers attached to
/// chapter/episode markers, ignore years, and accept decimals.
pub fn parse_chapter_number(name: &str) -> Option<f32> {
    let lower = name.to_lowercase();
    let bytes = lower.as_bytes();
    let mut best: Option<(u8, usize, f32)> = None;
    let mut index = 0;

    while index < bytes.len() {
        if !bytes[index].is_ascii_digit() {
            index += 1;
            continue;
        }

        let start = index;
        let mut seen_dot = false;
        while index < bytes.len() {
            let byte = bytes[index];
            if byte.is_ascii_digit() {
                index += 1;
            } else if byte == b'.'
                && !seen_dot
                && index + 1 < bytes.len()
                && bytes[index + 1].is_ascii_digit()
            {
                seen_dot = true;
                index += 1;
            } else {
                break;
            }
        }

        let Ok(value) = lower[start..index].parse::<f32>() else {
            continue;
        };
        let keyword = ends_with_chapter_keyword(&lower[..start]);

        let looks_like_year = index - start == 4
            && (lower[start..].starts_with("19") || lower[start..].starts_with("20"));
        if looks_like_year && !keyword {
            continue;
        }

        let priority = if keyword {
            3
        } else if start <= 2 {
            2
        } else {
            1
        };

        match best {
            None => best = Some((priority, start, value)),
            Some((best_priority, _, _)) if priority > best_priority => {
                best = Some((priority, start, value));
            }
            _ => {}
        }
    }

    best.map(|(_, _, value)| value)
}

fn ends_with_chapter_keyword(prefix: &str) -> bool {
    let prefix = prefix.trim_end_matches(|character: char| {
        character.is_whitespace() || matches!(character, '-' | '_' | '.' | '#' | '[' | '(')
    });

    if ["chapter", "chap", "episode", "ep"]
        .iter()
        .any(|keyword| prefix.ends_with(keyword))
    {
        return true;
    }

    // Single-letter markers: `c001`, `e05`, and `S2E05`. Reject words that merely
    // end in the letter, like "magic 5".
    ["ch", "c", "e"].iter().any(|keyword| {
        prefix.strip_suffix(keyword).is_some_and(|before| {
            before.is_empty()
                || !before
                    .chars()
                    .last()
                    .is_some_and(|character| character.is_alphabetic())
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_chapter_names() {
        assert_eq!(parse_chapter_number("Chapter 12"), Some(12.0));
        assert_eq!(parse_chapter_number("Chapter 12.5"), Some(12.5));
        assert_eq!(parse_chapter_number("Ch. 4"), Some(4.0));
        assert_eq!(parse_chapter_number("c001"), Some(1.0));
        assert_eq!(parse_chapter_number("Episode 7"), Some(7.0));
        assert_eq!(parse_chapter_number("S2E05"), Some(5.0));
        assert_eq!(parse_chapter_number("Vol. 2 Ch. 10"), Some(10.0));
        assert_eq!(parse_chapter_number("10"), Some(10.0));
    }

    #[test]
    fn ignores_years_and_missing_numbers() {
        assert_eq!(parse_chapter_number("2023 One Shot"), None);
        assert_eq!(parse_chapter_number("No numbers here"), None);
    }

    #[test]
    fn natural_sort_orders_numbers() {
        let mut names = vec!["Chapter 10", "Chapter 2", "Chapter 1"];
        names.sort_by(|left, right| natural_cmp(left, right));
        assert_eq!(names, ["Chapter 1", "Chapter 2", "Chapter 10"]);
    }
}
