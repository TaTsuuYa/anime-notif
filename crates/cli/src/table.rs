//! Plain-text table rendering for CLI output. Hand-rolled rather than a
//! table-formatting crate, since the output is a handful of simple
//! column-aligned reports and this keeps the dependency list small and the
//! formatting fully under test.

use anime_notif_core::config::CategoryDef;
use anime_notif_core::ExtractionResult;
use anime_notif_store::{InteractionRow, SeriesRow};

const DASH: &str = "-";

fn render(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.chars().count());
        }
    }

    let pad = |s: &str, w: usize| format!("{s:<w$}");
    let mut out = String::new();

    let header_line: Vec<String> = headers
        .iter()
        .zip(&widths)
        .map(|(h, w)| pad(h, *w))
        .collect();
    out.push_str(header_line.join("  ").trim_end());
    out.push('\n');

    let sep_line: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    out.push_str(&sep_line.join("  "));
    out.push('\n');

    for row in rows {
        let line: Vec<String> = row.iter().zip(&widths).map(|(c, w)| pad(c, *w)).collect();
        out.push_str(line.join("  ").trim_end());
        out.push('\n');
    }

    out
}

/// Renders the `list` table: ID, Name, Category, Last Episode, Alias, Last
/// Interaction.
pub fn format_series_table(rows: &[SeriesRow]) -> String {
    let headers = [
        "ID",
        "Name",
        "Category",
        "Last Episode",
        "Alias",
        "Last Interaction",
    ];
    if rows.is_empty() {
        return "(no shows tracked yet)\n".to_string();
    }
    let cells: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            vec![
                r.id.to_string(),
                r.title.clone(),
                r.category.clone(),
                r.last_episode.clone().unwrap_or_else(|| DASH.to_string()),
                r.alias.clone().unwrap_or_else(|| DASH.to_string()),
                r.last_interaction_at
                    .map(|t| t.to_rfc3339())
                    .unwrap_or_else(|| DASH.to_string()),
            ]
        })
        .collect();
    render(&headers, &cells)
}

/// Renders a single show's row plus its interaction history — the output
/// of `<selector> show`.
pub fn format_series_detail(row: &SeriesRow, history: &[InteractionRow]) -> String {
    let mut out = format_series_table(std::slice::from_ref(row));
    out.push_str("\nHistory:\n");
    if history.is_empty() {
        out.push_str("  (no recorded interactions)\n");
    }
    for h in history {
        out.push_str(&format!(
            "  {}  {}{}\n",
            h.at.to_rfc3339(),
            h.kind.as_str(),
            h.detail
                .as_ref()
                .map(|d| format!(" -> {d}"))
                .unwrap_or_default()
        ));
    }
    out
}

/// Renders `categories list`.
pub fn format_categories(categories: &[CategoryDef]) -> String {
    let headers = ["Name", "Notify", "Auto-download"];
    let cells: Vec<Vec<String>> = categories
        .iter()
        .map(|c| {
            vec![
                c.name.clone(),
                c.notify.to_string(),
                c.auto_download.to_string(),
            ]
        })
        .collect();
    render(&headers, &cells)
}

/// Renders `source list`.
pub fn format_sources(sources: &[String]) -> String {
    if sources.is_empty() {
        return "(no sources configured)\n".to_string();
    }
    sources.iter().map(|s| format!("- {s}\n")).collect()
}

/// Renders the output of `source test`: extracted releases plus any
/// extraction warnings.
pub fn format_extraction_result(result: &ExtractionResult) -> String {
    let mut out = format!("{} release(s) extracted\n", result.releases.len());
    for r in &result.releases {
        out.push_str(&format!(
            "- {} ep {} [{}] {} -> {}\n",
            r.series_title, r.episode, r.resolution, r.method, r.link
        ));
    }
    if !result.warnings.is_empty() {
        out.push_str(&format!("\n{} warning(s):\n", result.warnings.len()));
        for w in &result.warnings {
            out.push_str(&format!("- {w}\n"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use anime_notif_core::{DownloadMethod, Release};

    fn row(id: i64, title: &str, category: &str) -> SeriesRow {
        SeriesRow {
            id,
            source_id: "subsplease".into(),
            title: title.into(),
            alias: None,
            category: category.into(),
            last_episode: None,
            last_interaction_at: None,
            cover_url: None,
        }
    }

    #[test]
    fn empty_table_says_so() {
        assert_eq!(format_series_table(&[]), "(no shows tracked yet)\n");
    }

    #[test]
    fn table_has_header_and_one_line_per_row() {
        let rows = vec![row(1, "One Piece", "liked"), row(2, "Naruto", "normal")];
        let out = format_series_table(&rows);
        let lines: Vec<&str> = out.lines().collect();
        // header + separator + 2 rows
        assert_eq!(lines.len(), 4);
        assert!(lines[0].contains("ID"));
        assert!(lines[2].contains("One Piece"));
        assert!(lines[3].contains("Naruto"));
    }

    #[test]
    fn missing_fields_render_as_dash() {
        let out = format_series_table(&[row(1, "One Piece", "liked")]);
        assert!(out.contains(" - "));
    }

    #[test]
    fn extraction_result_lists_releases_then_warnings() {
        let result = ExtractionResult {
            releases: vec![Release {
                source_id: "subsplease".into(),
                series_title: "One Piece".into(),
                episode: "1121".into(),
                season: None,
                resolution: "1080".into(),
                method: DownloadMethod::Magnet,
                link: "magnet:?xt=aaa".into(),
                cover_url: None,
                show_url: None,
                raw_id: None,
            }],
            warnings: vec!["item 2: missing required field 'series'".into()],
        };
        let out = format_extraction_result(&result);
        assert!(out.contains("1 release(s) extracted"));
        assert!(out.contains("One Piece"));
        assert!(out.contains("1 warning(s)"));
        assert!(out.contains("missing required field"));
    }
}
