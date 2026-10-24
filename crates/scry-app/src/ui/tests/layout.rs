use super::*;

#[test]
fn sidebar_keeps_only_a_compact_logo_at_minimum_window_size() {
    let mut rt = rune::Runtime::new(init(), view, (880.0, 600.0), rune::Env::default());
    rt.frame(0.0);
    let elements = rt.elements();
    let header = rect(&rt, "topbar");
    let logo = rect(&rt, "brand-logo");
    assert!(
        elements
            .iter()
            .all(|element| element.text.as_deref() != Some("Scry"))
    );
    assert_eq!([logo[2], logo[3]], [28.0, 28.0]);
    assert!(logo[0] >= header[0] && logo[1] >= header[1]);
    assert!(logo[1] + logo[3] <= header[1] + header[3]);
    let back = rect(&rt, "nav-back");
    assert!(back[0] - (logo[0] + logo[2]) <= 24.0);
    assert!((logo[1] + logo[3] / 2.0 - (back[1] + back[3] / 2.0)).abs() < 1.0);
    assert!(rect(&rt, "nav-watching")[1] >= header[1] + header[3] + 14.0);
    assert_eq!(rect(&rt, "sidebar")[1], header[1] + header[3]);
}

#[test]
fn episode_wheel_is_bounded_and_sources_and_toolbar_stay_visible() {
    let mut state = init();
    state.navigation.page = Page::Detail;
    state.detail = Some(series(250));
    let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
    rt.frame(0.0);
    let toolbar = rect(&rt, "topbar");
    let viewport = rect(&rt, "episodes-scroll-0-0-Some(1)");
    let sources = rect(&rt, "sources-scroll-0");
    let first = rect(&rt, "episode-0");
    assert!(viewport[3] > 100.0 && viewport[1] + viewport[3] <= 600.0);
    assert!(sources[0] >= viewport[0] + viewport[2]);
    rt.wheel(viewport[0] + 20.0, viewport[1] + 20.0, -100_000.0);
    for _ in 0..120 {
        rt.frame(1.0 / 60.0);
    }
    let last = rect(&rt, "episode-99");
    assert!(
        rect(&rt, "episode-0")[1] < first[1] - 100.0,
        "first {first:?}, after {:?}, viewport {viewport:?}, last {last:?}",
        rect(&rt, "episode-0")
    );
    assert!(
        last[1] >= viewport[1] && last[1] + last[3] <= viewport[1] + viewport[3] + 1.0,
        "Last episode must be reachable: {last:?} in {viewport:?}"
    );
    assert_eq!(rect(&rt, "topbar"), toolbar);
    assert_eq!(rect(&rt, "sources-scroll-0"), sources);
}
