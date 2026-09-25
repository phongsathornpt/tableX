use super::{ExplorerRow, ObjectExplorer, group_tables};
use crate::domain::database_object::{TableRelationType, TableSummary};

#[test]
fn groups_a_page_once_by_schema_and_relation_type() {
    let groups = group_tables(vec![
        table("public", "users", TableRelationType::Table),
        table("public", "active_users", TableRelationType::View),
        table("audit", "events", TableRelationType::ForeignTable),
    ]);

    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].schema, "audit");
    assert_eq!(groups[0].categories[0].label, "Foreign tables");
    assert_eq!(groups[1].schema, "public");
    assert_eq!(
        groups[1]
            .categories
            .iter()
            .map(|category| category.label)
            .collect::<Vec<_>>(),
        ["Tables", "Views"]
    );
}

#[test]
fn flattened_virtual_rows_follow_schema_and_category_expansion() {
    let mut explorer = ObjectExplorer::new(vec!["public".into()]);
    explorer.table_groups = group_tables(vec![
        table("public", "users", TableRelationType::Table),
        table("public", "active_users", TableRelationType::View),
    ]);
    explorer.table_count = 2;
    explorer.rebuild_visible_rows();

    assert_eq!(explorer.visible_rows.len(), 4);
    assert_eq!(
        explorer
            .visible_rows
            .iter()
            .filter(|row| matches!(row, ExplorerRow::Table { .. }))
            .count(),
        1,
        "the default Tables category is expanded, while Views remains collapsed"
    );
    explorer
        .category_expansion
        .insert("public:Views".into(), true);
    explorer.rebuild_visible_rows();
    assert_eq!(
        explorer
            .visible_rows
            .iter()
            .filter(|row| matches!(row, ExplorerRow::Table { .. }))
            .count(),
        2
    );

    explorer.collapsed_schemas.insert("public".into());
    explorer.rebuild_visible_rows();
    assert!(matches!(
        explorer.visible_rows.as_slice(),
        [ExplorerRow::Schema {
            collapsed: true,
            ..
        }]
    ));
}

fn table(schema: &str, name: &str, relation_type: TableRelationType) -> TableSummary {
    TableSummary {
        id: format!("{schema}.{name}"),
        schema: schema.to_owned(),
        name: name.to_owned(),
        relation_type,
    }
}
