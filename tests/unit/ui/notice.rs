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
