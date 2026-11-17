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
