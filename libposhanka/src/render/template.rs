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

/// `body` markup, either raw (if `body_markup` is set and the text parses as
/// valid Pango markup) or escaped plain text otherwise.
///
/// The sender is responsible for well-formed Pango markup once
/// `body_markup` is negotiated (notred's `[notifications].body_markup`
/// capability); malformed markup falls back to escaped plain text rather
/// than failing the whole card, since Pango would otherwise reject the
/// entire layout for one bad tag.
fn body_markup_or_escaped(body: &str, body_markup: bool) -> String {
    if !body_markup {
        return pango_escape(body);
    }
    // Validate in isolation (wrapped in a throwaway root element) — `body`
    // is substituted into a larger template that may add its own tags
    // around it, so this only checks that `body` itself is well-formed.
    match pango::parse_markup(&format!("<span>{body}</span>"), '\0') {
        Ok(_) => body.to_string(),
        Err(_) => pango_escape(body),
    }
}

pub fn apply_template(template: &str, notification: &NotificationView) -> String {
    template
        .replace("{summary}", &pango_escape(&notification.summary))
        .replace(
            "{body}",
            &body_markup_or_escaped(&notification.body, notification.body_markup),
        )
        .replace("{app_id}", &pango_escape(&notification.app_id))
        .replace("{id}", &notification.id.to_string())
}
