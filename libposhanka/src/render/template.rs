use crate::model::NotificationView;

/// Escape plain text for safe insertion into Pango markup templates.
pub fn pango_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\'' => out.push_str("&apos;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

pub fn apply_template(template: &str, notification: &NotificationView) -> String {
    template
        .replace("{summary}", &pango_escape(&notification.summary))
        .replace("{body}", &pango_escape(&notification.body))
        .replace("{app_id}", &pango_escape(&notification.app_id))
        .replace("{id}", &notification.id.to_string())
}
