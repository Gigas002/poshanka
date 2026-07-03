use crate::model::NotificationView;

use super::{FeedMessage, ParseFeedError, RawNotification, parse_line};

/// Parse stdout from `[provider].command list` (wire NDJSON or CLI JSON array).
pub fn parse_list_output(stdout: &str) -> Result<Vec<NotificationView>, ParseFeedError> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    if trimmed.starts_with('[') {
        let raw: Vec<RawNotification> =
            serde_json::from_str(trimmed).map_err(ParseFeedError::Json)?;
        return raw.into_iter().map(NotificationView::try_from).collect();
    }

    if let Some(FeedMessage::Items(items)) = parse_line(trimmed)? {
        return Ok(items);
    }

    Ok(Vec::new())
}

#[cfg(test)]
mod tests;
