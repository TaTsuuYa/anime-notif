//! Pure decision logic for the resolution-wait workflow: ranking
//! resolutions against the fallback preference list, picking the best
//! available variant (preferring the favourite download method), and
//! matching a series' title against declarative rules to seed its
//! category. Kept free of I/O so it's exhaustively unit-testable.

use anime_notif_core::config::{CategoryDef, Rule};
use anime_notif_core::{DownloadMethod, Release};

/// Ranks `resolution` against `fallback`'s preference order: lower is
/// better (0 = most preferred). A resolution absent from `fallback` ranks
/// after every listed one.
pub fn rank_resolution(resolution: &str, fallback: &[String]) -> usize {
    fallback
        .iter()
        .position(|r| r == resolution)
        .unwrap_or(fallback.len())
}

/// Among `variants` (assumed non-empty, all for the same episode), picks
/// the best-ranked resolution per `fallback`, preferring `default_method`
/// among variants tied at that resolution.
pub fn best_variant<'a>(
    variants: &'a [Release],
    fallback: &[String],
    default_method: DownloadMethod,
) -> &'a Release {
    let best_rank = variants
        .iter()
        .map(|v| rank_resolution(&v.resolution, fallback))
        .min()
        .expect("variants is non-empty");
    let mut candidates = variants
        .iter()
        .filter(|v| rank_resolution(&v.resolution, fallback) == best_rank);
    let first = candidates
        .next()
        .expect("at least one candidate at best_rank");
    std::iter::once(first)
        .chain(candidates)
        .find(|v| v.method == default_method)
        .unwrap_or(first)
}

/// Finds the variant at exactly `desired_resolution`, preferring
/// `default_method` among ties, or `None` if no variant has that
/// resolution.
pub fn find_desired<'a>(
    variants: &'a [Release],
    desired_resolution: &str,
    default_method: DownloadMethod,
) -> Option<&'a Release> {
    let mut at_resolution = variants
        .iter()
        .filter(|v| v.resolution == desired_resolution);
    let first = at_resolution.next()?;
    Some(
        std::iter::once(first)
            .chain(at_resolution)
            .find(|v| v.method == default_method)
            .unwrap_or(first),
    )
}

/// Determines the category a newly-seen series should be assigned, by
/// matching `rules` in order (first match wins). Falls back to a category
/// named `"normal"` if defined, else the first defined category.
pub fn determine_category<'a>(
    rules: &[Rule],
    categories: &'a [CategoryDef],
    title: &str,
) -> &'a str {
    for rule in rules {
        let matched = if let Some(exact) = &rule.match_exact {
            exact == title
        } else if let Some(pattern) = &rule.match_regex {
            regex::Regex::new(pattern)
                .map(|re| re.is_match(title))
                .unwrap_or(false)
        } else {
            false
        };
        if matched {
            if let Some(c) = categories.iter().find(|c| c.name == rule.category) {
                return &c.name;
            }
        }
    }
    categories
        .iter()
        .find(|c| c.name == "normal")
        .or_else(|| categories.first())
        .map(|c| c.name.as_str())
        .unwrap_or("normal")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(resolution: &str, method: DownloadMethod) -> Release {
        Release {
            source_id: "subsplease".into(),
            series_title: "One Piece".into(),
            episode: "5".into(),
            season: None,
            resolution: resolution.into(),
            method,
            link: format!("link-{resolution}-{method}"),
            cover_url: None,
            raw_id: None,
        }
    }

    fn fallback() -> Vec<String> {
        vec!["1080".into(), "720".into(), "480".into()]
    }

    #[test]
    fn rank_resolution_orders_by_fallback_position() {
        let fb = fallback();
        assert_eq!(rank_resolution("1080", &fb), 0);
        assert_eq!(rank_resolution("480", &fb), 2);
        assert_eq!(rank_resolution("2160", &fb), 3); // not listed -> worst
    }

    #[test]
    fn best_variant_picks_highest_ranked_resolution() {
        let variants = vec![
            release("480", DownloadMethod::Magnet),
            release("720", DownloadMethod::Magnet),
        ];
        let best = best_variant(&variants, &fallback(), DownloadMethod::Magnet);
        assert_eq!(best.resolution, "720");
    }

    #[test]
    fn best_variant_prefers_default_method_among_ties() {
        let variants = vec![
            release("720", DownloadMethod::Torrent),
            release("720", DownloadMethod::Magnet),
        ];
        let best = best_variant(&variants, &fallback(), DownloadMethod::Magnet);
        assert_eq!(best.method, DownloadMethod::Magnet);
    }

    #[test]
    fn best_variant_falls_back_to_first_when_default_method_absent() {
        let variants = vec![release("720", DownloadMethod::Torrent)];
        let best = best_variant(&variants, &fallback(), DownloadMethod::Magnet);
        assert_eq!(best.method, DownloadMethod::Torrent);
    }

    #[test]
    fn unlisted_resolution_ranks_worse_than_listed_ones() {
        let variants = vec![
            release("2160", DownloadMethod::Magnet), // not in fallback list
            release("720", DownloadMethod::Magnet),
        ];
        let best = best_variant(&variants, &fallback(), DownloadMethod::Magnet);
        assert_eq!(best.resolution, "720");
    }

    #[test]
    fn find_desired_returns_none_when_absent() {
        let variants = vec![release("480", DownloadMethod::Magnet)];
        assert!(find_desired(&variants, "1080", DownloadMethod::Magnet).is_none());
    }

    #[test]
    fn find_desired_prefers_default_method() {
        let variants = vec![
            release("1080", DownloadMethod::Torrent),
            release("1080", DownloadMethod::Magnet),
        ];
        let found = find_desired(&variants, "1080", DownloadMethod::Magnet).unwrap();
        assert_eq!(found.method, DownloadMethod::Magnet);
    }

    fn categories() -> Vec<CategoryDef> {
        CategoryDef::defaults()
    }

    #[test]
    fn determine_category_matches_exact_rule() {
        let rules = vec![Rule {
            match_exact: Some("One Piece".into()),
            match_regex: None,
            category: "liked".into(),
        }];
        assert_eq!(
            determine_category(&rules, &categories(), "One Piece"),
            "liked"
        );
    }

    #[test]
    fn determine_category_matches_regex_rule() {
        let rules = vec![Rule {
            match_exact: None,
            match_regex: Some("^Isekai.*".into()),
            category: "uninterested".into(),
        }];
        assert_eq!(
            determine_category(&rules, &categories(), "Isekai Cheat Slime"),
            "uninterested"
        );
        assert_eq!(
            determine_category(&rules, &categories(), "One Piece"),
            "normal"
        );
    }

    #[test]
    fn determine_category_defaults_to_normal_when_no_rule_matches() {
        assert_eq!(determine_category(&[], &categories(), "Anything"), "normal");
    }

    #[test]
    fn determine_category_first_rule_wins() {
        let rules = vec![
            Rule {
                match_exact: Some("One Piece".into()),
                match_regex: None,
                category: "liked".into(),
            },
            Rule {
                match_regex: Some(".*".into()),
                match_exact: None,
                category: "uninterested".into(),
            },
        ];
        assert_eq!(
            determine_category(&rules, &categories(), "One Piece"),
            "liked"
        );
    }
}
