use super::{FeedSignal, NotificationState};
use crate::model::{NotificationView, Urgency};

fn sample(id: u32) -> NotificationView {
    NotificationView {
        id,
        app_id: "app".into(),
        summary: "s".into(),
        body: "b".into(),
        urgency: Urgency::Normal,
        timeout_ms: None,
        has_actions: false,
    }
}

#[test]
fn apply_items_replaces_snapshot() {
    let mut state = NotificationState::default();
    let kind = state.apply_signal(FeedSignal::Items(vec![sample(1), sample(2)]));
    assert_eq!(kind, super::FeedSignalKind::Items);
    assert_eq!(state.len(), 2);
}

#[test]
fn reload_signal_does_not_change_items() {
    let mut state = NotificationState::default();
    state.replace(vec![sample(1)]);
    let kind = state.apply_signal(FeedSignal::Reload);
    assert_eq!(kind, super::FeedSignalKind::Reload);
    assert_eq!(state.len(), 1);
}
