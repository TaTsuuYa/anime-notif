//! Resolves a user-typed selector (numeric id, alias, or title) to a show,
//! and a user-typed category name/prefix to a defined category — the two
//! places the CLI needs "shortest thing that's still unambiguous" matching.

use anime_notif_core::config::CategoryDef;
use anime_notif_store::{SeriesRow, Store};

use crate::error::CliError;

/// Resolves `input` to a single show, trying, in order: numeric id, exact
/// alias, exact title. A title matching more than one show (across
/// sources) is reported as ambiguous rather than guessing — nothing is
/// changed in that case, per the CLI's documented duplicate-name handling.
pub async fn resolve_selector(store: &Store, input: &str) -> Result<SeriesRow, CliError> {
    if let Ok(id) = input.parse::<i64>() {
        if let Some(row) = store.get_by_id(id).await? {
            return Ok(row);
        }
    }
    if let Some(row) = store.get_by_alias(input).await? {
        return Ok(row);
    }
    let matches = store.find_by_title(input).await?;
    match matches.len() {
        0 => Err(CliError::NotFound(input.to_string())),
        1 => Ok(matches.into_iter().next().unwrap()),
        n => Err(CliError::AmbiguousName(input.to_string(), n)),
    }
}

/// Resolves `input` to a defined category's name: an exact match always
/// wins outright; otherwise the shortest-unique-prefix match (so `l` works
/// when only `liked` starts with `l`, and the caller is told to type more
/// of the name — e.g. two letters — when it doesn't).
pub fn resolve_category<'a>(
    input: &str,
    categories: &'a [CategoryDef],
) -> Result<&'a str, CliError> {
    if let Some(c) = categories.iter().find(|c| c.name == input) {
        return Ok(c.name.as_str());
    }
    let matches: Vec<&str> = categories
        .iter()
        .map(|c| c.name.as_str())
        .filter(|name| name.starts_with(input))
        .collect();
    match matches.len() {
        0 => Err(CliError::UnknownCategory(input.to_string())),
        1 => Ok(matches[0]),
        _ => Err(CliError::AmbiguousCategory(
            input.to_string(),
            matches.join(", "),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn categories() -> Vec<CategoryDef> {
        vec![
            CategoryDef {
                name: "liked".into(),
                notify: true,
                auto_download: true,
            },
            CategoryDef {
                name: "normal".into(),
                notify: true,
                auto_download: false,
            },
            CategoryDef {
                name: "uninterested".into(),
                notify: false,
                auto_download: false,
            },
        ]
    }

    #[test]
    fn resolves_by_unambiguous_single_letter() {
        let cats = categories();
        assert_eq!(resolve_category("l", &cats).unwrap(), "liked");
        assert_eq!(resolve_category("n", &cats).unwrap(), "normal");
        assert_eq!(resolve_category("u", &cats).unwrap(), "uninterested");
    }

    #[test]
    fn resolves_exact_match_even_if_a_prefix_of_another() {
        let mut cats = categories();
        cats.push(CategoryDef {
            name: "liked-extra".into(),
            notify: true,
            auto_download: true,
        });
        assert_eq!(resolve_category("liked", &cats).unwrap(), "liked");
    }

    #[test]
    fn ambiguous_prefix_lists_candidates() {
        let mut cats = categories();
        cats.push(CategoryDef {
            name: "notified".into(),
            notify: true,
            auto_download: false,
        });
        let err = resolve_category("n", &cats).unwrap_err();
        match err {
            CliError::AmbiguousCategory(input, candidates) => {
                assert_eq!(input, "n");
                assert!(candidates.contains("normal"));
                assert!(candidates.contains("notified"));
            }
            other => panic!("expected AmbiguousCategory, got {other:?}"),
        }
        // "no" is still ambiguous (both start with it); three letters
        // where the names actually diverge disambiguate.
        assert!(matches!(
            resolve_category("no", &cats),
            Err(CliError::AmbiguousCategory(_, _))
        ));
        assert_eq!(resolve_category("nor", &cats).unwrap(), "normal");
        assert_eq!(resolve_category("not", &cats).unwrap(), "notified");
    }

    #[test]
    fn unknown_category_is_reported() {
        let cats = categories();
        assert!(matches!(
            resolve_category("xyz", &cats),
            Err(CliError::UnknownCategory(_))
        ));
    }

    #[tokio::test]
    async fn selector_precedence_id_then_alias_then_title() {
        let store = Store::open_in_memory().await.unwrap();
        let a = store
            .upsert_series("subsplease", "One Piece", "normal", None)
            .await
            .unwrap();
        store.set_alias(a.id, "op").await.unwrap();

        assert_eq!(
            resolve_selector(&store, &a.id.to_string())
                .await
                .unwrap()
                .id,
            a.id
        );
        assert_eq!(resolve_selector(&store, "op").await.unwrap().id, a.id);
        assert_eq!(
            resolve_selector(&store, "One Piece").await.unwrap().id,
            a.id
        );
    }

    #[tokio::test]
    async fn duplicate_title_is_ambiguous_and_changes_nothing() {
        let store = Store::open_in_memory().await.unwrap();
        store
            .upsert_series("subsplease", "One Piece", "normal", None)
            .await
            .unwrap();
        store
            .upsert_series("nyaa", "One Piece", "normal", None)
            .await
            .unwrap();

        let err = resolve_selector(&store, "One Piece").await.unwrap_err();
        assert!(matches!(err, CliError::AmbiguousName(_, 2)));
    }

    #[tokio::test]
    async fn unknown_selector_is_not_found() {
        let store = Store::open_in_memory().await.unwrap();
        let err = resolve_selector(&store, "nope").await.unwrap_err();
        assert!(matches!(err, CliError::NotFound(_)));
    }
}
