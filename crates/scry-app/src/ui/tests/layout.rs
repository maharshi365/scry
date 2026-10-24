use super::*;

#[test]
fn sidebar_keeps_the_logo_and_wordmark_visible_at_minimum_window_size() {
    let mut rt = rune::Runtime::new(init(), view, (880.0, 600.0), rune::Env::default());
    rt.frame(0.0);
    let elements = rt.elements();
    let sidebar = rect(&rt, "sidebar");
    let logo = rect(&rt, "brand-logo");
    let wordmark = elements
        .iter()
        .find(|element| element.text.as_deref() == Some("SKRY"))
        .expect("Skry wordmark")
        .rect;
    assert_eq!([logo[2], logo[3]], [28.0, 28.0]);
    for bounds in [logo, wordmark] {
        assert!(bounds[0] >= sidebar[0] && bounds[1] >= sidebar[1]);
        assert!(bounds[0] + bounds[2] <= sidebar[0] + sidebar[2]);
        assert!(bounds[1] + bounds[3] <= sidebar[1] + sidebar[3]);
    }
    assert!(
        wordmark[0] >= logo[0] + logo[2],
        "wordmark must not overlap the logo"
    );
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
