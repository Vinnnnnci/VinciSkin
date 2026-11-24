use super::*;

#[test]
fn a_runtime_below_the_build_target_is_too_old() {
    assert!(webview2_too_old("104.0.1293.70"));
    assert!(webview2_too_old("77.0.0.0"));
}

#[test]
fn a_runtime_at_or_above_the_build_target_is_fine() {
    assert!(!webview2_too_old("105.0.0.0"));
    assert!(!webview2_too_old("158.0.4370.33"));
}

#[test]
fn a_version_that_does_not_parse_is_not_called_old() {
    assert!(!webview2_too_old(""));
    assert!(!webview2_too_old("unknown"));
}
