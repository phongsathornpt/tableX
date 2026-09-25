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
mod tests {
    use super::{Notice, NoticeLevel};

    #[test]
    fn keeps_user_message_separate_from_technical_detail() {
        let notice = Notice::error("Connection failed", "Could not connect")
            .with_detail("certificate verify failed");

        assert_eq!(notice.level, NoticeLevel::Error);
        assert_eq!(notice.title, "Connection failed");
        assert_eq!(notice.message, "Could not connect");
        assert_eq!(notice.detail.as_deref(), Some("certificate verify failed"));
    }

    #[test]
    fn assigns_expected_levels_to_shortcuts() {
        assert_eq!(Notice::info("working").level, NoticeLevel::Info);
        assert_eq!(Notice::success("done").level, NoticeLevel::Success);
        assert_eq!(
            Notice::warning("title", "check").level,
            NoticeLevel::Warning
        );
    }
}
