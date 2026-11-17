//! poshanka end to end in a headless sway (see `common`).

mod common;

use common::{BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, Poshanka, card_origin, notification};

/// A point inside the `index`th card.
fn inside(index: u32) -> (u32, u32) {
    let (x, y) = card_origin(index);
    (x + 100, y + 20)
}

#[test]
fn a_left_click_closes_a_card_or_runs_its_action() {
    let sway = sway_or_skip!();
    let mut poshanka = Poshanka::start(&sway);
    let mut pointer = poshanka.pointer(&sway);
    poshanka.show(&[
        notification(1, "plain", ""),
        notification(2, "with actions", r#""has_actions": true"#),
    ]);

    let (x, y) = inside(0);
    pointer.click_at(x, y, BTN_LEFT);
    poshanka.wait_for_call("close 1");

    let (x, y) = inside(1);
    pointer.click_at(x, y, BTN_LEFT);
    poshanka.wait_for_call("activate 2");
}

#[test]
fn right_and_middle_clicks_are_reported_as_input() {
    let sway = sway_or_skip!();
    let mut poshanka = Poshanka::start(&sway);
    let mut pointer = poshanka.pointer(&sway);
    poshanka.show(&[notification(5, "hello", "")]);

    let (x, y) = inside(0);
    pointer.click_at(x, y, BTN_RIGHT);
    poshanka.wait_for_call("input 5 button_right");
    pointer.click_at(x, y, BTN_MIDDLE);
    poshanka.wait_for_call("input 5 button_middle");
}

#[test]
fn clicks_between_cards_do_nothing() {
    let sway = sway_or_skip!();
    let mut poshanka = Poshanka::start(&sway);
    let mut pointer = poshanka.pointer(&sway);
    poshanka.show(&[notification(1, "a", ""), notification(2, "b", "")]);

    // In the gap between the two cards, then on the second one.
    let (x, y) = card_origin(1);
    pointer.click_at(x + 100, y - common::GAP / 2, BTN_LEFT);
    pointer.click_at(x + 100, y + 20, BTN_LEFT);
    poshanka.wait_for_call("close 2");
    assert!(
        !poshanka.calls().contains("close 1"),
        "{}",
        poshanka.calls()
    );
}

#[test]
fn the_initial_list_is_used_before_the_feed_says_anything() {
    let sway = sway_or_skip!();
    let dir = tempfile::tempdir().unwrap();
    // The list is read from `list.json` beside the fake notredctl, which
    // `Poshanka::start` creates; write it first by starting in two steps.
    let config =
        common::CONFIG.replace("{dir}/notredctl", &format!("{}/ctl", dir.path().display()));
    std::fs::write(
        dir.path().join("ctl"),
        format!(
            "#!/bin/sh\necho \"$*\" >> '{}/calls.log'\n[ \"$1\" = list ] && echo '[{}]'\nexit 0\n",
            dir.path().display(),
            notification(7, "from list", ""),
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(
        dir.path().join("ctl"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();

    let mut poshanka = Poshanka::start_with(&sway, &config, common::THEME, &[]);
    poshanka.wait_for_log("the initial list", "initial provider list count=1");
    let mut pointer = poshanka.pointer(&sway);
    poshanka.wait_for_log("the paint", "stack painted");
    let (x, y) = inside(0);
    pointer.click_at(x, y, BTN_LEFT);
    common::wait_for("close 7", || {
        std::fs::read_to_string(dir.path().join("calls.log"))
            .ok()?
            .lines()
            .any(|l| l == "close 7")
            .then_some(())
    });
}

#[test]
fn a_lone_card_is_drawn_and_cleared() {
    let sway = sway_or_skip!();
    let mut poshanka = Poshanka::start(&sway);
    poshanka.show(&[notification(1, "only one", "")]);
    let card = common::wait_for_card(&sway, 0, true);
    assert_eq!(card.pixel(0, common::CARD_H / 2), common::BORDER);

    poshanka.update(&[]);
    common::wait_for_card(&sway, 0, false);

    // A new surface after the stack was empty.
    poshanka.show(&[notification(2, "again", "")]);
    common::wait_for_card(&sway, 0, true);
}

#[test]
fn every_card_of_a_stack_is_drawn() {
    let sway = sway_or_skip!();
    let mut poshanka = Poshanka::start(&sway);
    poshanka.show(&[
        notification(1, "a", ""),
        notification(2, "b", ""),
        notification(3, "c", ""),
    ]);
    for index in 0..3 {
        common::wait_for_card(&sway, index, true);
    }
    // The gaps between cards stay clear.
    let (x, y) = card_origin(1);
    let gap = sway.screenshot(x, y - common::GAP, common::CARD_W, common::GAP);
    assert_eq!(gap.count(common::CARD_BG), 0);
}

/// Every repaint creates a buffer from a new shm pool; both must be given
/// back to the compositor, or it keeps their memory for as long as poshanka
/// runs.
#[test]
fn buffers_and_pools_are_destroyed_after_use() {
    let sway = sway_or_skip!();
    let mut poshanka = Poshanka::start_with(
        &sway,
        common::CONFIG,
        common::THEME,
        &[("WAYLAND_DEBUG", "1")],
    );
    for i in 0..20 {
        poshanka.show(&[notification(1, &format!("update {i}"), "")]);
    }
    poshanka.update(&[]);
    let log = poshanka.log();
    let requests = |object: &str, request: &str| {
        log.lines()
            .filter(|l| l.contains(&format!("-> {object}@")) && l.contains(request))
            .count()
    };
    let created = requests("wl_shm_pool", ".create_buffer(");
    assert!(created >= 20, "{created} buffers created");
    // Pools right away; buffers once the compositor releases them, or with
    // their surface.
    assert_eq!(
        requests("wl_shm_pool", ".destroy()"),
        created,
        "pools destroyed"
    );
    let destroyed = requests("wl_buffer", ".destroy()");
    assert_eq!(
        destroyed, created,
        "{destroyed} of {created} buffers destroyed"
    );
}

/// Updates that resize the stack faster than the compositor answers: the
/// answer to an older size must not replace the newest frame.
#[test]
fn a_burst_of_updates_ends_with_every_card_drawn() {
    let sway = sway_or_skip!();
    let mut poshanka = Poshanka::start(&sway);
    let mut items = Vec::new();
    // 12 cards: all on the 720 px output.
    for round in 0..2 {
        for i in 1..=6 {
            items.push(notification(round * 6 + i, "burst", ""));
            poshanka.send(&format!(
                r#"{{"v": 1, "type": "event", "event": {{"kind": "update", "items": [{}]}}}}"#,
                items.join(", ")
            ));
        }
        let n = items.len() as u32;
        for index in [0, n / 2, n - 1] {
            common::wait_for_card(&sway, index, true);
        }
    }
}

/// One-channel image data used to index past the pixel buffer and crash
/// poshanka; now the card falls back to another icon.
#[test]
fn an_unreadable_image_does_not_crash_poshanka() {
    let sway = sway_or_skip!();
    let theme = common::THEME.replace("size = 0", "size = 16");
    let mut poshanka = Poshanka::start_with(&sway, common::CONFIG, &theme, &[]);
    let icon = r#""icon": {"width": 4, "height": 4, "rowstride": 4, "has_alpha": false, "bits_per_sample": 8, "channels": 1, "data": "AAAAAAAAAAAAAAAAAAAAAA=="}"#;
    poshanka.show(&[notification(1, "gray avatar", icon)]);
    common::wait_for_card(&sway, 0, true);
    assert!(
        poshanka.try_wait().is_none(),
        "poshanka exited:\n{}",
        poshanka.log()
    );
}

/// A feed script that delivers and then exits (a provider restart) is run
/// again after a second each time, not after ever longer pauses.
#[test]
fn a_feed_that_keeps_delivering_is_restarted_promptly() {
    let sway = sway_or_skip!();
    let dir = tempfile::tempdir().unwrap();
    let starts = dir.path().join("starts");
    let script = dir.path().join("feed.sh");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\necho >> '{}'\necho '{{\"v\": 1, \"type\": \"event\", \"event\": {{\"kind\": \"update\", \"items\": []}}}}'\n",
            starts.display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let config = common::CONFIG.replace("{dir}/feed.sh", script.to_str().unwrap());

    let _poshanka = Poshanka::start_with(&sway, &config, common::THEME, &[]);
    // With a growing backoff the fifth start comes after 1+2+4+8 s.
    std::thread::sleep(std::time::Duration::from_secs(7));
    let runs = std::fs::read_to_string(&starts).unwrap().lines().count();
    assert!(runs >= 5, "{runs} runs in 7 s");
}

/// The feed script must not outlive poshanka, however poshanka ends.
#[test]
fn the_feed_script_ends_with_poshanka() {
    let sway = sway_or_skip!();
    let mut poshanka = Poshanka::start(&sway);
    let pid = common::wait_for("the feed script", || {
        std::fs::read_to_string(poshanka.path("feed.pid"))
            .ok()
            .filter(|s| s.ends_with('\n'))
    });
    let proc = std::path::Path::new("/proc").join(pid.trim());
    assert!(proc.exists());
    poshanka.kill();
    common::wait_for("the feed script to end", || (!proc.exists()).then_some(()));
}
