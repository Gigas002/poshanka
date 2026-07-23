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
        progress_mode: ProgressMode::Over,
    }
}

fn test_notification() -> NotificationView {
    NotificationView {
        id: 1,
        app_id: "firefox".into(),
        summary: "Hello".into(),
        body: "This is a notification body.".into(),
        urgency: Urgency::Normal,
        timeout_ms: Some(10_000),
        has_actions: false,
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
    let frame = paint_card(&style, &notification, &font).expect("paint");

    let px = pixel_bgra(&frame.data, frame.stride, 20, 20);
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
    let y = (block.y + 4.0) as u32;
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
fn painted_card_icon_placeholder_is_visible() {
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
fn template_escapes_markup_in_user_text() {
    let style = test_style();
    let mut notification = test_notification();
    notification.summary = "a <b>bold</b> claim".into();
    let font = FontContext::new(&style.font_name, style.font_size).expect("font");
    let computed = measure_card(&style, &notification, &font).expect("measure");
    assert!(computed.blocks[0].markup.contains("&lt;b&gt;"));
}
