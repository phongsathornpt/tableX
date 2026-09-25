#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TableRelationType {
    #[default]
    Table,
    PartitionedTable,
    View,
    MaterializedView,
    ForeignTable,
}

impl TableRelationType {
    pub fn label(self) -> &'static str {
        match self {
            Self::Table => "Table",
            Self::PartitionedTable => "Partitioned table",
            Self::View => "View",
            Self::MaterializedView => "Materialized view",
            Self::ForeignTable => "Foreign table",
        }
    }

    pub(crate) fn relkind(self) -> &'static str {
        match self {
            Self::Table => "r",
            Self::PartitionedTable => "p",
            Self::View => "v",
            Self::MaterializedView => "m",
            Self::ForeignTable => "f",
        }
    }

    pub(crate) fn from_relkind(value: &str) -> Option<Self> {
        match value {
            "r" => Some(Self::Table),
            "p" => Some(Self::PartitionedTable),
            "v" => Some(Self::View),
            "m" => Some(Self::MaterializedView),
            "f" => Some(Self::ForeignTable),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableSummary {
    pub id: String,
    pub schema: String,
    pub name: String,
    pub relation_type: TableRelationType,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableCursor {
    pub schema: String,
    pub table: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TablePage {
    pub tables: Vec<TableSummary>,
    pub limit: usize,
    pub offset: usize,
    pub has_next: bool,
    pub next_cursor: Option<TableCursor>,
}
