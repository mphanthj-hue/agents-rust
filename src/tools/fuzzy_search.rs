use strsim::levenshtein;

const FUZZY_THRESHOLD: f64 = 0.7;

#[derive(Debug)]
pub struct FuzzyMatch {
    pub start: usize,
    #[allow(dead_code)]
    pub end: usize,
    pub value: String,
    #[allow(dead_code)]
    pub distance: usize,
    pub similarity: f64,
}

pub fn find_closest(text: &str, query: &str) -> FuzzyMatch {
    let text_chars: Vec<char> = text.chars().collect();
    let query_chars: Vec<char> = query.chars().collect();
    let query_len = query_chars.len();

    if query_len == 0 || text_chars.len() < query_len {
        return FuzzyMatch {
            start: 0,
            end: 0,
            value: String::new(),
            distance: query_len,
            similarity: 0.0,
        };
    }

    let mut best_distance = usize::MAX;
    let mut best_start = 0;
    let mut best_end = query_len;

    let search_range = text_chars.len() - query_len;
    let step = if search_range > 1000 { search_range / 1000 } else { 1 };

    let mut i = 0;
    while i <= search_range {
        let end = std::cmp::min(i + query_len * 2, text_chars.len());
        let candidate: String = text_chars[i..end].iter().collect();
        let dist = levenshtein(&candidate, query);

        if dist < best_distance {
            best_distance = dist;
            best_start = i;
            best_end = end;

            if dist == 0 {
                break;
            }
        }

        i += step;
    }

    // Refine: slide window to find better boundaries
    let refine_range = std::cmp::max(query_len * 2, best_end - best_start);
    let refine_start = best_start.saturating_sub(refine_range);
    let refine_end = std::cmp::min(best_end + refine_range, text_chars.len());

    for i in refine_start..=refine_end.saturating_sub(query_len) {
        for j in (i + query_len)..=std::cmp::min(i + query_len * 2, refine_end) {
            let candidate: String = text_chars[i..j].iter().collect();
            let dist = levenshtein(&candidate, query);
            if dist < best_distance {
                best_distance = dist;
                best_start = i;
                best_end = j;
            }
        }
    }

    let value: String = text_chars[best_start..best_end].iter().collect();
    let max_len = std::cmp::max(value.len(), query_len);
    let similarity = if max_len == 0 {
        1.0
    } else {
        1.0 - (best_distance as f64 / max_len as f64)
    };

    FuzzyMatch {
        start: best_start,
        end: best_end,
        value,
        distance: best_distance,
        similarity,
    }
}

pub fn highlight_diff(expected: &str, actual: &str) -> String {
    let common_prefix = expected.chars()
        .zip(actual.chars())
        .take_while(|(a, b)| a == b)
        .count();

    let common_suffix = expected.chars().rev()
        .zip(actual.chars().rev())
        .take_while(|(a, b)| a == b)
        .count();

    let prefix: String = expected.chars().take(common_prefix).collect();
    let expected_mid: String = expected.chars()
        .skip(common_prefix)
        .take(expected.len() - common_prefix - common_suffix)
        .collect();
    let actual_mid: String = actual.chars()
        .skip(common_prefix)
        .take(actual.len() - common_prefix - common_suffix)
        .collect();
    let suffix: String = expected.chars()
        .skip(expected.len() - common_suffix)
        .collect();

    format!("{}{{-{}-}}{{+{}+}}{}", prefix, expected_mid, actual_mid, suffix)
}

pub fn is_similar_enough(similarity: f64) -> bool {
    similarity >= FUZZY_THRESHOLD
}
