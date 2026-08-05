use crate::model::{CardStyle, IconPos, NotificationView, ProgressMode, TextAlign, Urgency};
use crate::render::{FontContext, measure_card, paint_card};

fn pixel_bgra(data: &[u8], stride: i32, x: u32, y: u32) -> [u8; 4] {
    let offset = y as usize * stride as usize + x as usize * 4;
    let mut px = [0u8; 4];
    px.copy_from_slice(&data[offset..offset + 4]);
    px
}

fn test_style() -> CardStyle {
    CardStyle {
        background_bgra: [0x77, 0x55, 0x28, 0xff],
        foreground_bgra: [0xff, 0xff, 0xff, 0xff],
        border_bgra: [0x99, 0x78, 0x4c, 0xff],
        progress_bgra: [0xaa, 0x88, 0x55, 0xff],
        font_name: "sans-serif".into(),
        font_size: 14.0,
        width: 300,
        height: 120,
        padding: 8,
        margin: 0,
        border_size: 2,
        border_radius: 8,
        text_alignment: TextAlign::Left,
        summary_template: "<b>{summary}</b>".into(),
        body_template: "{body}".into(),
        app_template: None,
        id_template: None,
        icon_size: 48,
        icon_position: IconPos::Left,
        icon_theme: String::new(),
        icon_default_name: String::new(),
        progress_mode: ProgressMode::Over,
        progress_height: 4,
    }
}

fn test_notification() -> NotificationView {
    NotificationView {
        id: 1,
        app_id: "poshanka-test-app-id-that-does-not-exist".into(),
        summary: "Hello".into(),
        body: "This is a notification body.".into(),
        urgency: Urgency::Normal,
        timeout_ms: Some(10_000),
        has_actions: false,
        icon: None,
        progress: None,
        category: None,
        desktop_entry: None,
        body_markup: false,
    }
}

#[test]
fn measure_card_fits_within_max_dimensions() {
    let style = test_style();
    let notification = test_notification();
    let font = FontContext::new(&style.font_name, style.font_size).expect("font");
    let computed = measure_card(&style, &notification, &font).expect("measure");
    assert_eq!(computed.width, style.width);
    assert!(computed.height <= style.height);
    assert!(computed.height > 0);
    assert_eq!(computed.blocks.len(), 2);
    assert!(computed.icon.is_some());
}

#[test]
fn measure_card_shrinks_for_short_content() {
    let mut style = test_style();
    style.height = 400;
    let mut notification = test_notification();
    notification.body.clear();
    let font = FontContext::new(&style.font_name, style.font_size).expect("font");
    let computed = measure_card(&style, &notification, &font).expect("measure");
    assert!(computed.height < style.height);
}

#[test]
fn painted_card_has_background_pixel() {
    let style = test_style();
    let notification = test_notification();
    let font = FontContext::new(&style.font_name, style.font_size).expect("font");
    let computed = measure_card(&style, &notification, &font).expect("measure");
    let frame = paint_card(&style, &notification, &font).expect("paint");

    // Sample just below both the icon and the text stack (whichever extends
    // further), inside the bottom padding band and above the border stroke —
    // background regardless of exactly how tall the icon/text content is.
    let content_bottom = computed
        .icon
        .as_ref()
        .map(|icon| icon.y + icon.size)
        .into_iter()
        .chain(computed.blocks.iter().map(|b| b.y + b.height))
        .fold(0.0_f64, f64::max);
    let y = ((content_bottom + 2.0) as u32).min(frame.height.saturating_sub(3));
    let px = pixel_bgra(&frame.data, frame.stride, 20, y);
    assert_eq!(px, style.background_bgra);
}

#[test]
fn painted_card_has_foreground_text_pixel() {
    let style = test_style();
    let notification = test_notification();
    let font = FontContext::new(&style.font_name, style.font_size).expect("font");
    let computed = measure_card(&style, &notification, &font).expect("measure");
    let frame = paint_card(&style, &notification, &font).expect("paint");

    let block = &computed.blocks[0];
    let x = (block.x + 4.0) as u32;
    // Vertical middle of the line box, not a fixed offset from its top —
    // robust to how much Pango leading/ascent sits above the glyph ink,
    // which shifts as `block.y` moves with vertical centering.
    let y = (block.y + block.height / 2.0) as u32;
    let px = pixel_bgra(
        &frame.data,
        frame.stride,
        x.min(frame.width.saturating_sub(1)),
        y.min(frame.height.saturating_sub(1)),
    );
    assert_ne!(px, style.background_bgra);
    assert_eq!(px[3], 0xff);
}

#[test]
fn no_progress_rect_without_progress_value() {
    let style = test_style();
    let notification = test_notification();
    let font = FontContext::new(&style.font_name, style.font_size).expect("font");
    let computed = measure_card(&style, &notification, &font).expect("measure");
    assert!(computed.progress.is_none());
}

#[test]
fn no_progress_rect_when_height_is_zero() {
    let mut style = test_style();
    style.progress_height = 0;
    let mut notification = test_notification();
    notification.progress = Some(50);
    let font = FontContext::new(&style.font_name, style.font_size).expect("font");
    let computed = measure_card(&style, &notification, &font).expect("measure");
    assert!(computed.progress.is_none());
}

#[test]
fn progress_rect_reserves_space_within_card_bounds() {
    let style = test_style();
    let mut notification = test_notification();
    notification.progress = Some(40);
    let font = FontContext::new(&style.font_name, style.font_size).expect("font");
    let computed = measure_card(&style, &notification, &font).expect("measure");

    let progress = computed.progress.expect("progress rect");
    assert!((progress.fraction - 0.4).abs() < f64::EPSILON);
    assert!(
        progress.y + progress.height <= f64::from(computed.height),
        "progress bar (y={}, height={}) extends past card height {}",
        progress.y,
        progress.height,
        computed.height
    );
    // Bottom-aligned within the card's inner content area.
    let origin = f64::from(style.border_size + style.padding);
    assert!((progress.y + progress.height - (f64::from(computed.height) - origin)).abs() < 0.5);
}

#[test]
fn progress_value_out_of_range_clamps_to_full_range() {
    let style = test_style();
    let mut over = test_notification();
    over.progress = Some(150);
    let mut under = test_notification();
    under.progress = Some(-10);
    let font = FontContext::new(&style.font_name, style.font_size).expect("font");

    let computed_over = measure_card(&style, &over, &font).expect("measure");
    assert!((computed_over.progress.unwrap().fraction - 1.0).abs() < f64::EPSILON);

    let computed_under = measure_card(&style, &under, &font).expect("measure");
    assert!((computed_under.progress.unwrap().fraction - 0.0).abs() < f64::EPSILON);
}

#[test]
fn painted_card_has_progress_fill_pixel() {
    let style = test_style();
    let mut notification = test_notification();
    notification.progress = Some(100);
    let font = FontContext::new(&style.font_name, style.font_size).expect("font");
    let computed = measure_card(&style, &notification, &font).expect("measure");
    let frame = paint_card(&style, &notification, &font).expect("paint");

    let progress = computed.progress.expect("progress rect");
    let x = (progress.x + 4.0) as u32;
    let y = (progress.y + progress.height / 2.0) as u32;
    let px = pixel_bgra(&frame.data, frame.stride, x, y);
    assert_eq!(px, style.progress_bgra);
}

#[test]
fn icon_stays_within_card_bounds_when_shorter_than_icon() {
    // Reproduces the reported bug: a large icon (relative to a tall
    // configured max-height) combined with short text used to center the
    // icon against the *max* height instead of the actual shrink-to-fit
    // height, pushing it below the real card bounds.
    let mut style = test_style();
    style.height = 300;
    style.icon_size = 128;
    let mut notification = test_notification();
    notification.body = "Short.".into();
    let font = FontContext::new(&style.font_name, style.font_size).expect("font");
    let computed = measure_card(&style, &notification, &font).expect("measure");

    let icon = computed.icon.expect("icon rect");
    assert!(
        icon.y + icon.size <= f64::from(computed.height),
        "icon (y={}, size={}) extends past card height {}",
        icon.y,
        icon.size,
        computed.height
    );
    assert!(icon.y >= 0.0);
}

#[test]
fn text_stack_centers_vertically_when_icon_taller_than_text() {
    let mut style = test_style();
    style.height = 300;
    style.icon_size = 128;
    let mut notification = test_notification();
    notification.body = "Short.".into();
    let font = FontContext::new(&style.font_name, style.font_size).expect("font");
    let computed = measure_card(&style, &notification, &font).expect("measure");

    let origin = f64::from(style.border_size + style.padding);
    let first_block_y = computed.blocks[0].y;
    assert!(
        first_block_y > origin,
        "text block should be pushed down from the top edge to center within \
         the icon-driven card height, got y={first_block_y} (origin={origin})"
    );
}

#[test]
fn painted_card_icon_placeholder_is_visible() {
    // icon_default_name is empty in test_style() (fallback disabled), so
    // with no icon sent and an app_id/desktop_entry that resolves to
    // nothing, only the plain placeholder square is left.
    let style = test_style();
    let notification = test_notification();
    let font = FontContext::new(&style.font_name, style.font_size).expect("font");
    let computed = measure_card(&style, &notification, &font).expect("measure");
    let frame = paint_card(&style, &notification, &font).expect("paint");

    let icon = computed.icon.expect("icon rect");
    let x = (icon.x + icon.size / 2.0) as u32;
    let y = (icon.y + icon.size / 2.0) as u32;
    let px = pixel_bgra(&frame.data, frame.stride, x, y);
    assert_eq!(px, style.progress_bgra);
}

#[test]
#[cfg(feature = "icons")]
fn painted_card_uses_configured_default_icon_when_nothing_else_resolves() {
    // Neither icon.name/path/raw nor the app_id/desktop_entry fallback
    // resolve, but theme.toml's `icons.default` name does — that icon
    // should be used instead of the accent-colored placeholder square.
    let dir = tempfile::tempdir().unwrap();
    let apps_dir = dir.path().join("scalable/status");
    std::fs::create_dir_all(&apps_dir).unwrap();
    let icon_path = apps_dir.join("my-configured-default.svg");
    std::fs::write(
        &icon_path,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="#ff0000"/></svg>"##,
    )
    .unwrap();

    let mut style = test_style();
    style.icon_theme = dir.path().to_string_lossy().into_owned();
    style.icon_default_name = "my-configured-default".into();
    let notification = test_notification();

    let font = FontContext::new(&style.font_name, style.font_size).expect("font");
    let computed = measure_card(&style, &notification, &font).expect("measure");
    let frame = paint_card(&style, &notification, &font).expect("paint");

    let icon = computed.icon.expect("icon rect");
    let x = (icon.x + icon.size / 2.0) as u32;
    let y = (icon.y + icon.size / 2.0) as u32;
    let px = pixel_bgra(&frame.data, frame.stride, x, y);
    assert_ne!(
        px, style.progress_bgra,
        "expected the configured default icon, not the placeholder"
    );
    assert_eq!(
        (px[0], px[1], px[2], px[3]),
        (0, 0, 255, 255),
        "opaque red in BGRA order"
    );
}

#[test]
#[cfg(feature = "icons")]
fn painted_card_uses_desktop_entry_icon_instead_of_placeholder() {
    // No icon.name/path/raw was sent, but the notification has a
    // desktop_entry hint that resolves in the icon theme — that app icon
    // should be painted instead of the accent-colored placeholder square.
    let dir = tempfile::tempdir().unwrap();
    let apps_dir = dir.path().join("scalable/apps");
    std::fs::create_dir_all(&apps_dir).unwrap();
    let icon_path = apps_dir.join("some-app.svg");
    std::fs::write(
        &icon_path,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="#ff0000"/></svg>"##,
    )
    .unwrap();

    let mut style = test_style();
    style.icon_theme = dir.path().to_string_lossy().into_owned();
    let mut notification = test_notification();
    notification.desktop_entry = Some("some-app".into());

    let font = FontContext::new(&style.font_name, style.font_size).expect("font");
    let computed = measure_card(&style, &notification, &font).expect("measure");
    let frame = paint_card(&style, &notification, &font).expect("paint");

    let icon = computed.icon.expect("icon rect");
    let x = (icon.x + icon.size / 2.0) as u32;
    let y = (icon.y + icon.size / 2.0) as u32;
    let px = pixel_bgra(&frame.data, frame.stride, x, y);
    assert_ne!(
        px, style.progress_bgra,
        "expected the resolved app icon, not the placeholder"
    );
    assert_eq!(
        (px[0], px[1], px[2], px[3]),
        (0, 0, 255, 255),
        "opaque red in BGRA order"
    );
}

#[test]
fn template_escapes_markup_in_user_text() {
    let style = test_style();
    let mut notification = test_notification();
    notification.summary = "a <b>bold</b> claim".into();
    let font = FontContext::new(&style.font_name, style.font_size).expect("font");
    let computed = measure_card(&style, &notification, &font).expect("measure");
    assert!(computed.blocks[0].markup.contains("&lt;b&gt;"));
}

#[test]
fn body_is_escaped_when_body_markup_is_false() {
    let style = test_style();
    let mut notification = test_notification();
    notification.body = "<b>bold</b> body".into();
    notification.body_markup = false;
    let font = FontContext::new(&style.font_name, style.font_size).expect("font");
    let computed = measure_card(&style, &notification, &font).expect("measure");
    assert!(computed.blocks[1].markup.contains("&lt;b&gt;"));
}

#[test]
fn body_renders_as_raw_markup_when_body_markup_is_true_and_valid() {
    let style = test_style();
    let mut notification = test_notification();
    notification.body = "<b>bold</b> body".into();
    notification.body_markup = true;
    let font = FontContext::new(&style.font_name, style.font_size).expect("font");
    let computed = measure_card(&style, &notification, &font).expect("measure");
    assert!(computed.blocks[1].markup.contains("<b>bold</b>"));
    assert!(!computed.blocks[1].markup.contains("&lt;b&gt;"));
}

#[test]
fn body_falls_back_to_escaped_when_body_markup_is_malformed() {
    let style = test_style();
    let mut notification = test_notification();
    // Unbalanced tag — not valid Pango markup.
    notification.body = "<b>unclosed bold".into();
    notification.body_markup = true;
    let font = FontContext::new(&style.font_name, style.font_size).expect("font");
    let computed = measure_card(&style, &notification, &font).expect("measure");
    assert!(computed.blocks[1].markup.contains("&lt;b&gt;"));
}
