use crate::model::NotificationView;

use super::FeedSignal;

/// In-memory snapshot of visible notifications from the provider feed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NotificationState {
    items: Vec<NotificationView>,
}

impl NotificationState {
    pub fn items(&self) -> &[NotificationView] {
        &self.items
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn replace(&mut self, items: Vec<NotificationView>) {
        self.items = items;
    }

    pub fn apply_signal(&mut self, signal: FeedSignal) -> FeedSignalKind {
        match signal {
            FeedSignal::Items(items) => {
                self.replace(items);
                FeedSignalKind::Items
            }
            FeedSignal::Reload => FeedSignalKind::Reload,
        }
    }
}

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedSignalKind {
    Items,
    Reload,
}
