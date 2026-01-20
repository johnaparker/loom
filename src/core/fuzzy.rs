//! Fuzzy matching utilities for TUI and CLI commands.
//!
//! Provides a unified interface for fuzzy searching using the nucleo library.

use nucleo::{Config as NucleoConfig, Matcher, Utf32Str};

/// A scored item with its original index and fuzzy match score.
#[derive(Debug, Clone)]
pub struct ScoredItem {
    pub index: usize,
    pub score: u16,
}

/// Fuzzy matcher for searching items by string content.
///
/// This struct wraps the nucleo `Matcher` and provides convenient methods
/// for common fuzzy matching operations.
pub struct FuzzyMatcher {
    matcher: Matcher,
}

impl Default for FuzzyMatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl FuzzyMatcher {
    /// Create a new fuzzy matcher with default configuration.
    pub fn new() -> Self {
        Self {
            matcher: Matcher::new(NucleoConfig::DEFAULT),
        }
    }

    /// Score a single haystack against a needle.
    ///
    /// Returns the score if there's a match, or None if no match.
    pub fn score(&mut self, haystack: &str, needle: &str) -> Option<u16> {
        let mut haystack_buf = Vec::new();
        let haystack_str = Utf32Str::new(haystack, &mut haystack_buf);
        let mut needle_buf = Vec::new();
        let needle_str = Utf32Str::new(needle, &mut needle_buf);

        self.matcher.fuzzy_match(haystack_str, needle_str)
    }

    /// Find the best matching item from a collection.
    ///
    /// The `to_haystack` function extracts the searchable string from each item.
    /// Returns `None` if no items match the query.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let items = vec!["apple", "banana", "apricot"];
    /// let mut matcher = FuzzyMatcher::new();
    /// let best = matcher.best_match(&items, "apr", |s| s.to_string());
    /// assert_eq!(best, Some(2)); // "apricot" is the best match
    /// ```
    pub fn best_match<T, F>(&mut self, items: &[T], query: &str, to_haystack: F) -> Option<usize>
    where
        F: Fn(&T) -> String,
    {
        let scored = self.score_all(items, query, to_haystack);
        scored.first().map(|s| s.index)
    }

    /// Score all items and return sorted results (best matches first).
    ///
    /// The `to_haystack` function extracts the searchable string from each item.
    /// Only items that match the query are returned, sorted by score descending.
    pub fn score_all<T, F>(&mut self, items: &[T], query: &str, to_haystack: F) -> Vec<ScoredItem>
    where
        F: Fn(&T) -> String,
    {
        let mut scored: Vec<ScoredItem> = items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| {
                let haystack = to_haystack(item);
                self.score(&haystack, query).map(|score| ScoredItem { index: i, score })
            })
            .collect();

        // Sort by score descending
        scored.sort_by(|a, b| b.score.cmp(&a.score));
        scored
    }

    /// Filter items by query and return indices sorted by match score.
    ///
    /// This is a convenience method for TUI filtering where you need
    /// the indices of matching items in score order.
    ///
    /// If the query is empty, returns all indices in original order.
    pub fn filter<T, F>(&mut self, items: &[T], query: &str, to_haystack: F) -> Vec<usize>
    where
        F: Fn(&T) -> String,
    {
        if query.is_empty() {
            return (0..items.len()).collect();
        }

        self.score_all(items, query, to_haystack)
            .into_iter()
            .map(|s| s.index)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_best_match() {
        let items = vec!["apple", "banana", "apricot", "grape"];
        let mut matcher = FuzzyMatcher::new();

        // "apr" should match "apricot" best
        let best = matcher.best_match(&items, "apr", |s| s.to_string());
        assert_eq!(best, Some(2));

        // "ban" should match "banana" best
        let best = matcher.best_match(&items, "ban", |s| s.to_string());
        assert_eq!(best, Some(1));

        // No match
        let best = matcher.best_match(&items, "xyz", |s| s.to_string());
        assert_eq!(best, None);
    }

    #[test]
    fn test_filter_empty_query() {
        let items = vec!["a", "b", "c"];
        let mut matcher = FuzzyMatcher::new();

        let indices = matcher.filter(&items, "", |s| s.to_string());
        assert_eq!(indices, vec![0, 1, 2]);
    }

    #[test]
    fn test_filter_with_query() {
        let items = vec!["feature-login", "bugfix-auth", "feature-logout"];
        let mut matcher = FuzzyMatcher::new();

        let indices = matcher.filter(&items, "feature", |s| s.to_string());
        assert_eq!(indices.len(), 2);
        // Both feature items should match
        assert!(indices.contains(&0));
        assert!(indices.contains(&2));
    }

    #[test]
    fn test_score() {
        let mut matcher = FuzzyMatcher::new();

        // Should match
        assert!(matcher.score("hello world", "helo").is_some());

        // Should not match
        assert!(matcher.score("hello", "xyz").is_none());
    }
}
