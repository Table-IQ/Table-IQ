//! Shared by the live tests (each opts in with `mod common;`).

/// The value of `var`, or None when it is unset so the test skips itself. With
/// `TIQ_REQUIRE_LIVE=1` an unset one panics instead: CI must not pass by skipping.
pub fn live_url(var: &str) -> Option<String> {
    let v = std::env::var(var).ok();
    if v.is_none() {
        assert!(std::env::var("TIQ_REQUIRE_LIVE").as_deref() != Ok("1"), "{var} unset and TIQ_REQUIRE_LIVE=1");
        eprintln!("SKIP: {var} unset");
    }
    v
}
