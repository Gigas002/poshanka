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
/// than failing the whole card, since the markup parser would otherwise
/// reject the entire block for one bad tag.
fn body_markup_or_escaped(body: &str, body_markup: bool) -> String {
    if !body_markup {
        return pango_escape(body);
    }
    // Validate in isolation (wrapped in a throwaway root element) — `body`
    // is substituted into a larger template that may add its own tags
    // around it, so this only checks that `body` itself is well-formed.
    match super::markup::validate(&format!("<span>{body}</span>")) {
        Ok(_) => body.to_string(),
        Err(_) => pango_escape(body),
    }
}

/// Fill `{summary}`, `{body}`, `{app_id}` and `{id}` in `template` in one
/// pass, so placeholders inside the inserted text stay literal. Other
/// `{…}` text is kept as it is.
pub fn apply_template(template: &str, notification: &NotificationView) -> String {
    let mut out = String::with_capacity(template.len() + notification.body.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let tail = &rest[open..];
        let close = tail.find('}').map_or(tail.len(), |i| i + 1);
        match &tail[..close] {
            "{summary}" => out.push_str(&pango_escape(&notification.summary)),
            "{body}" => out.push_str(&body_markup_or_escaped(
                &notification.body,
                notification.body_markup,
            )),
            "{app_id}" => out.push_str(&pango_escape(&notification.app_id)),
            "{id}" => out.push_str(&notification.id.to_string()),
            // Not a placeholder: keep the `{` and go on after it, so a
            // placeholder right behind it (`{{id}`) is still found.
            _ => {
                out.push('{');
                rest = &tail[1..];
                continue;
            }
        }
        rest = &tail[close..];
    }
    out.push_str(rest);
    out
}
