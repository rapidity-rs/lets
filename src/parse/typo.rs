//! Typo detection for misspelled KDL keywords.

/// Known command child node keywords.
const KNOWN_KEYWORDS: &[&str] = &[
    "description",
    "long-description",
    "examples",
    "hide",
    "deprecated",
    "run",
    "arg",
    "flag",
    "deps",
    "steps",
    "before",
    "after",
    "env",
    "env-file",
    "dir",
    "shell",
    "platform",
    "run-macos",
    "run-linux",
    "run-windows",
    "confirm",
    "prompt",
    "choose",
    "alias",
    "timeout",
    "retry",
    "silent",
    "quiet",
    "cmd",
];

/// Check if an unrecognized node name is a likely typo of a known keyword.
/// Returns the closest keyword if the edit distance is small relative to word length.
pub(super) fn check_typo(name: &str) -> Option<&'static str> {
    let max_dist = if name.len() <= 4 { 1 } else { 2 };
    let mut best: Option<(&str, usize)> = None;
    for &kw in KNOWN_KEYWORDS {
        let dist = edit_distance(name, kw);
        let len_diff = name.len().abs_diff(kw.len());
        if dist <= max_dist
            && dist > 0
            && len_diff <= 2
            && (best.is_none() || dist < best.unwrap().1)
        {
            best = Some((kw, dist));
        }
    }
    best.map(|(kw, _)| kw)
}

/// Simple Levenshtein edit distance.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());
    let mut prev = (0..=n).collect::<Vec<_>>();
    let mut curr = vec![0; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}
