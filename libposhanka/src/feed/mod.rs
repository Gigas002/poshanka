use serde::Deserialize;

use crate::model::{NotificationView, Urgency};

/// Parsed NDJSON line from a provider feed script stdout (list response or subscribe event).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedMessage {
    Items(Vec<NotificationView>),
    Event(FeedEvent),
}

/// Event pushed on a provider subscribe stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedEvent {
    Update { items: Vec<NotificationView> },
    Reload,
    HistoryChanged,
}

/// Parse one NDJSON line from provider feed stdout.
pub fn parse_line(line: &str) -> Result<Option<FeedMessage>, ParseFeedError> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(None);
    }

    let raw: RawLine = serde_json::from_str(line).map_err(ParseFeedError::Json)?;

    match raw.msg_type.as_deref() {
        Some("items") => {
            let items = raw
                .items
                .unwrap_or_default()
                .into_iter()
                .map(NotificationView::try_from)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Some(FeedMessage::Items(items)))
        }
        Some("event") => {
            let event = raw
                .event
                .ok_or(ParseFeedError::MissingField { field: "event" })?;
            Ok(Some(FeedMessage::Event(event.try_into()?)))
        }
        // subscribe handshake and command replies are ignored by the feed parser.
        None => Ok(None),
        Some(other) => Err(ParseFeedError::UnknownType(other.to_string())),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ParseFeedError {
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("missing field `{field}`")]
    MissingField { field: &'static str },
    #[error("unknown message type `{0}`")]
    UnknownType(String),
    #[error("unknown event kind `{0}`")]
    UnknownEventKind(String),
    #[error("unknown urgency `{0}`")]
    UnknownUrgency(String),
}

#[derive(Debug, Deserialize)]
struct RawLine {
    #[serde(rename = "type")]
    msg_type: Option<String>,
    #[serde(default)]
    items: Option<Vec<RawNotification>>,
    event: Option<RawEvent>,
}

#[derive(Debug, Deserialize)]
struct RawEvent {
    kind: String,
    #[serde(default)]
    items: Vec<RawNotification>,
}

#[derive(Debug, Deserialize)]
struct RawNotification {
    id: u32,
    app_id: String,
    summary: String,
    #[serde(default)]
    body: String,
    urgency: String,
    timeout_ms: Option<u64>,
    #[serde(default)]
    has_actions: bool,
}

impl TryFrom<RawEvent> for FeedEvent {
    type Error = ParseFeedError;

    fn try_from(raw: RawEvent) -> Result<Self, Self::Error> {
        match raw.kind.as_str() {
            "update" => {
                let items = raw
                    .items
                    .into_iter()
                    .map(NotificationView::try_from)
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Self::Update { items })
            }
            "reload" => Ok(Self::Reload),
            "history_changed" => Ok(Self::HistoryChanged),
            other => Err(ParseFeedError::UnknownEventKind(other.to_string())),
        }
    }
}

impl TryFrom<RawNotification> for NotificationView {
    type Error = ParseFeedError;

    fn try_from(raw: RawNotification) -> Result<Self, Self::Error> {
        Ok(Self {
            id: raw.id,
            app_id: raw.app_id,
            summary: raw.summary,
            body: raw.body,
            urgency: parse_urgency(&raw.urgency)?,
            timeout_ms: raw.timeout_ms,
            has_actions: raw.has_actions,
        })
    }
}

fn parse_urgency(raw: &str) -> Result<Urgency, ParseFeedError> {
    match raw {
        "low" => Ok(Urgency::Low),
        "normal" => Ok(Urgency::Normal),
        "critical" => Ok(Urgency::Critical),
        other => Err(ParseFeedError::UnknownUrgency(other.to_string())),
    }
}

#[cfg(test)]
mod tests;
