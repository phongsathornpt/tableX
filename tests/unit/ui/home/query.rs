use super::{
    AsciiCaseInsensitiveMatcher, TABLE_CELL_PREVIEW_CHARS, contains_ascii_case_insensitive,
    is_inline_editable_cell, matching_row_indices, matching_row_indices_with_cancellation,
    result_cell_value, table_column_width, visible_cell_prefix,
};
use crate::domain::query::{EditableTable, QueryResult};

#[test]
fn matches_ascii_without_allocating_a_lowercased_copy() {
    assert!(contains_ascii_case_insensitive("Audit_Logs", "AUDIT"));
    assert!(contains_ascii_case_insensitive("éAudit", "éAUDIT"));
    assert!(!contains_ascii_case_insensitive("Audit", "logs"));
    assert!(contains_ascii_case_insensitive("anything", ""));
}

#[test]
fn compiled_filter_matcher_preserves_ascii_insensitive_substring_semantics() {
    for (value, pattern) in [
        ("Audit_Logs", "AUDIT"),
        ("éAudit", "éAUDIT"),
        ("abababac", "ababac"),
        ("anything", ""),
        ("Audit", "logs"),
    ] {
        assert_eq!(
            AsciiCaseInsensitiveMatcher::new(pattern).contains(value),
            contains_ascii_case_insensitive(value, pattern),
            "value={value:?}, pattern={pattern:?}"
        );
    }
}

#[test]
#[ignore = "filter matcher microbenchmark; invoke explicitly with --ignored"]
fn benchmarks_long_near_match_filter() {
    use std::time::Instant;

    let values = (0..12).map(|_| "a".repeat(16 * 1024)).collect::<Vec<_>>();
    let pattern = format!("{}b", "a".repeat(63));
    let matcher = AsciiCaseInsensitiveMatcher::new(&pattern);
    let mut old_times = Vec::with_capacity(9);
    let mut new_times = Vec::with_capacity(9);
    for _ in 0..9 {
        let started = Instant::now();
        let old_matches = values
            .iter()
            .filter(|value| contains_ascii_case_insensitive(value, &pattern))
            .count();
        old_times.push(started.elapsed());
        let started = Instant::now();
        let new_matches = values
            .iter()
            .filter(|value| matcher.contains(value))
            .count();
        new_times.push(started.elapsed());
        assert_eq!(old_matches, new_matches);
    }
    old_times.sort_unstable();
    new_times.sort_unstable();
    let old_median = old_times[4];
    let new_median = new_times[4];
    let speedup = old_median.as_secs_f64() / new_median.as_secs_f64();
    eprintln!(
        "long near-match filter over 12 x 16 KiB strings: window scan={:.2} ms, compiled matcher={:.2} ms, speedup={speedup:.1}x",
        old_median.as_secs_f64() * 1000.0,
        new_median.as_secs_f64() * 1000.0,
    );
}

#[test]
fn long_cell_preview_stops_on_a_character_boundary() {
    let value = format!("{}étail", "a".repeat(160));
    assert_eq!(visible_cell_prefix(&value), Some(&value[..160]));
    assert!(visible_cell_prefix("short value").is_none());
    assert!(visible_cell_prefix(&"é".repeat(80)).is_none());
}

#[test]
fn tooltip_lookup_returns_the_full_untruncated_result_cell() {
    let value = format!("{}tail", "x".repeat(TABLE_CELL_PREVIEW_CHARS + 1));
    let result = QueryResult {
        columns: vec!["large_text".into()],
        column_types: vec!["text".into()],
        column_enum_values: vec![None],
        rows: vec![vec![value.clone()]],
        null_cells: vec![vec![false]],
        truncated_cells: vec![vec![false]],
        offset: 0,
        limit: 1,
        has_next: false,
        truncated: false,
        editable: None,
    };

    assert_eq!(result_cell_value(&result, 0, 0), Some(value.as_str()));
    assert_eq!(result_cell_value(&result, 1, 0), None);
}

#[test]
fn truncated_result_cells_are_not_inline_editable() {
    let mut result = QueryResult {
        columns: vec!["payload".into()],
        column_types: vec!["jsonb".into()],
        column_enum_values: vec![None],
        rows: vec![vec!["{\"partial\":".into()]],
        null_cells: vec![vec![false]],
        truncated_cells: vec![vec![false]],
        offset: 0,
        limit: 1,
        has_next: false,
        truncated: false,
        editable: Some(EditableTable {
            schema: "public".into(),
            table: "documents".into(),
            primary_key_columns: vec!["id".into()],
        }),
    };

    assert!(is_inline_editable_cell(&result, 0, 0));
    result.truncated_cells[0][0] = true;
    assert!(!is_inline_editable_cell(&result, 0, 0));
}

#[test]
fn assigns_column_widths_without_case_normalization() {
    assert_eq!(table_column_width("USER_EMAIL", "text"), 210.);
    assert_eq!(
        table_column_width("created_AT", "timestamp without time zone"),
        230.
    );
    assert_eq!(table_column_width("is_ACTIVE", "BOOLEAN"), 112.);
    assert_eq!(table_column_width("Status", "text"), 140.);
}

#[test]
fn avoids_building_filtered_indices_without_active_filters() {
    let result = QueryResult {
        columns: vec!["name".into()],
        column_types: vec!["text".into()],
        column_enum_values: vec![None],
        rows: vec![vec!["first".into()], vec!["second".into()]],
        null_cells: vec![vec![false], vec![false]],
        truncated_cells: vec![vec![false], vec![false]],
        offset: 0,
        limit: 2,
        has_next: false,
        truncated: false,
        editable: None,
    };

    assert_eq!(
        matching_row_indices(&result, None, None, None, usize::MAX),
        None
    );
    assert_eq!(
        matching_row_indices(&result, Some("OND"), Some("name"), None, usize::MAX),
        Some(vec![1])
    );
    assert_eq!(
        matching_row_indices(&result, Some("OND"), Some("missing"), None, usize::MAX),
        Some(vec![])
    );
}

#[test]
fn filtered_row_matching_can_be_cancelled_and_respects_zero_limit() {
    let result = QueryResult {
        columns: vec!["name".into()],
        column_types: vec!["text".into()],
        column_enum_values: vec![None],
        rows: vec![vec!["first".into()], vec!["second".into()]],
        null_cells: vec![vec![false], vec![false]],
        truncated_cells: vec![vec![false], vec![false]],
        offset: 0,
        limit: 2,
        has_next: false,
        truncated: false,
        editable: None,
    };
    let mut rows_checked = 0;

    assert_eq!(
        matching_row_indices_with_cancellation(&result, Some("i"), None, None, usize::MAX, || {
            rows_checked += 1;
            rows_checked <= 1
        },),
        None
    );
    assert_eq!(
        matching_row_indices_with_cancellation(&result, Some("i"), None, None, 0, || panic!(
            "zero visible rows should not scan input"
        ),),
        Some(vec![])
    );
}
