//! Integration test with a live notification provider (notred + notredctl).
//!
//! Run manually when notred is running:
//! `NOTREDCTL=notredctl cargo test -p libposhanka --test live_provider -- --ignored`

use libposhanka::{ProviderSpec, fetch_list};

#[test]
#[ignore = "requires live provider daemon"]
fn live_list_returns_items() {
    let spec = ProviderSpec {
        command: std::env::var("NOTREDCTL")
            .ok()
            .or_else(|| Some("notredctl".into())),
        socket: std::env::var("NOTRED_SOCKET").ok(),
        ..Default::default()
    };
    let _items = fetch_list(&spec).expect("list from live provider");
}
