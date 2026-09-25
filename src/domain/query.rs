#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub column_types: Vec<String>,
    pub column_enum_values: Vec<Option<Vec<String>>>,
    pub rows: Vec<Vec<String>>,
    pub null_cells: Vec<Vec<bool>>,
    pub truncated_cells: Vec<Vec<bool>>,
    pub offset: usize,
    pub limit: usize,
    pub has_next: bool,
    pub truncated: bool,
    pub editable: Option<EditableTable>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditableTable {
    pub schema: String,
    pub table: String,
    pub primary_key_columns: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableDataCursorDirection {
    After,
    Before,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableDataCursor {
    pub values: Vec<String>,
    pub direction: TableDataCursorDirection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TablePreviewPageRequest {
    pub limit: usize,
    pub offset: usize,
    pub sort: Option<(String, bool)>,
    pub primary_key_columns: Vec<String>,
    pub cursor: Option<TableDataCursor>,
    pub filters: Vec<TableColumnFilter>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableFilterOperator {
    Equals,
    NotEquals,
    Contains,
    StartsWith,
    GreaterThan,
    LessThan,
    IsNull,
    IsNotNull,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableColumnFilter {
    pub column: String,
    pub operator: TableFilterOperator,
    pub value: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellUpdateRequest {
    pub schema: String,
    pub table: String,
    pub column: String,
    pub primary_key_columns: Vec<String>,
    pub primary_key_values: Vec<String>,
    pub value: Option<String>,
    pub expected_value: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MutationResult {
    pub affected_rows: u64,
}
