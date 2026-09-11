#[path = "../src/navigation/mod.rs"]
pub mod navigation;

use navigation::{Navigator, Route};

#[test]
fn navigation_preserves_back_history() {
    let mut navigator = Navigator::default();

    navigator.navigate(Route::Nearby);
    navigator.navigate(Route::Pairing);

    assert_eq!(navigator.current(), Route::Pairing);
    assert!(navigator.back());
    assert_eq!(navigator.current(), Route::Nearby);
}

#[test]
fn navigating_to_the_current_route_does_not_add_history() {
    let mut navigator = Navigator::default();

    navigator.navigate(Route::Welcome);

    assert!(!navigator.back());
}
