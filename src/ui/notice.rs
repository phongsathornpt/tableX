#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NoticeLevel {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Notice {
    pub(crate) level: NoticeLevel,
    pub(crate) title: String,
    pub(crate) message: String,
    pub(crate) detail: Option<String>,
}

impl Notice {
    pub(crate) fn info(message: impl Into<String>) -> Self {
        Self::new(NoticeLevel::Info, "Working", message, None)
    }

    pub(crate) fn success(message: impl Into<String>) -> Self {
        Self::new(NoticeLevel::Success, "Done", message, None)
    }

    pub(crate) fn warning(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(NoticeLevel::Warning, title, message, None)
    }

    pub(crate) fn error(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(NoticeLevel::Error, title, message, None)
    }

    pub(crate) fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    fn new(
        level: NoticeLevel,
        title: impl Into<String>,
        message: impl Into<String>,
        detail: Option<String>,
    ) -> Self {
        Self {
            level,
            title: title.into(),
            message: message.into(),
            detail,
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/ui/notice.rs"]
mod tests;
