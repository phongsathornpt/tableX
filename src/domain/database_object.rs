#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DatabaseObjectKind {
    Server,
    Database,
    Schema,
    Table,
    View,
    Function,
    Extension,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseObject {
    pub id: String,
    pub label: String,
    pub kind: DatabaseObjectKind,
    pub children: Vec<DatabaseObject>,
}

impl DatabaseObject {
    pub fn branch(
        id: impl Into<String>,
        label: impl Into<String>,
        kind: DatabaseObjectKind,
        children: Vec<Self>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            kind,
            children,
        }
    }

    pub fn leaf(id: impl Into<String>, label: impl Into<String>, kind: DatabaseObjectKind) -> Self {
        Self::branch(id, label, kind, Vec::new())
    }
}
