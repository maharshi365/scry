use super::*;
use crate::ui::{
    design::tokens,
    primitives::{button::Button, input::input},
};

#[test]
fn dropdown_variants_compose_with_sizes_and_gate_disabled_actions() {
    use crate::ui::primitives::button::{DropdownVariant, Size, dropdown_trigger};
    struct Control {
        variant: DropdownVariant,
        size: Size,
        enabled: bool,
        presses: usize,
    }
    fn control(s: &Control, _: &mut rune::Cx<Control>) -> rune::El {
        rune::col().child(
            dropdown_trigger("choice", "Choose", s.variant)
                .size(s.size)
                .enabled(s.enabled)
                .on_press(|s: &mut Control| s.presses += 1),
        )
    }
    for variant in [DropdownVariant::Transparent, DropdownVariant::Framed] {
        for (size, height, radius) in [
            (
                Size::Default,
                tokens::control::HEIGHT,
                tokens::control::RADIUS,
            ),
            (
                Size::Compact,
                tokens::control::COMPACT_HEIGHT,
                tokens::control::RADIUS,
            ),
            (
                Size::Setting,
                tokens::control::SETTING_HEIGHT,
                tokens::control::RADIUS,
            ),
            (
                Size::Pill,
                tokens::control::PILL_HEIGHT,
                tokens::control::PILL_RADIUS,
            ),
        ] {
            for enabled in [false, true] {
                let mut rt = rune::Runtime::new(
                    Control {
                        variant,
                        size,
                        enabled,
                        presses: 0,
                    },
                    control,
                    (400.0, 300.0),
                    rune::Env::default(),
                );
                rt.frame(0.0);
                let element = rt
                    .elements()
                    .into_iter()
                    .find(|e| e.key.as_deref() == Some("choice"))
                    .unwrap();
                assert_eq!(element.rect[3], height);
                assert_eq!(element.radius, radius);
                assert_eq!(
                    element.fill,
                    match variant {
                        DropdownVariant::Transparent => tokens::transparent(),
                        DropdownVariant::Framed => tokens::panel(),
                    }
                );
                assert!(rt.find("choice-chevron").is_some());
                let (x, y) = rt.find("choice").unwrap();
                rt.pointer_down(x, y);
                rt.pointer_up(x, y);
                rt.frame(0.0);
                assert_eq!(rt.state.presses, usize::from(enabled));
            }
        }
    }
}

#[test]
fn busy_buttons_keep_their_target_size_and_block_activation() {
    struct Control {
        busy: bool,
        presses: usize,
    }
    fn control(s: &Control, _: &mut rune::Cx<Control>) -> rune::El {
        rune::col().child(
            Button::new("save", "Save")
                .busy(s.busy)
                .on_press(|s: &mut Control| s.presses += 1),
        )
    }
    let mut rt = rune::Runtime::new(
        Control {
            busy: false,
            presses: 0,
        },
        control,
        (400.0, 300.0),
        rune::Env::default(),
    );
    rt.frame(0.0);
    let target = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("save"))
        .unwrap()
        .rect;
    rt.update(|s, _| s.busy = true);
    rt.frame(0.0);
    let busy = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("save"))
        .unwrap();
    assert_eq!(busy.rect, target);
    assert!(!busy.focusable);
    assert!(rt.find("save-spinner").is_some());
    rt.pointer_down(target[0] + target[2] / 2.0, target[1] + target[3] / 2.0);
    rt.pointer_up(target[0] + target[2] / 2.0, target[1] + target[3] / 2.0);
    rt.frame(0.0);
    assert_eq!(rt.state.presses, 0);
    rt.update(|s, _| s.busy = false);
    rt.frame(0.0);
    let (x, y) = rt.find("save").unwrap();
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    assert_eq!(rt.state.presses, 1);
}

#[test]
fn shared_progress_bars_bound_values_and_keep_accessible_percentages() {
    struct Bars {
        values: Vec<(&'static str, f64)>,
    }
    fn bars(s: &Bars, _: &mut rune::Cx<Bars>) -> rune::El {
        let mut bars = rune::col().w(200.0);
        for (key, value) in &s.values {
            bars = bars.child(crate::ui::primitives::progress::bar(key, *value));
        }
        bars
    }
    let mut rt = rune::Runtime::new(
        Bars {
            values: vec![
                ("partial", 0.25),
                ("negative", -0.5),
                ("overflow", 1.5),
                ("unknown", f64::NAN),
                ("infinite", f64::INFINITY),
            ],
        },
        bars,
        (400.0, 300.0),
        rune::Env::default(),
    );
    rt.frame(0.0);
    let elements = rt.elements();
    for (key, percent) in [
        ("partial", 25.0),
        ("negative", 0.0),
        ("overflow", 100.0),
        ("unknown", 0.0),
        ("infinite", 0.0),
    ] {
        let track = elements
            .iter()
            .find(|e| e.key.as_deref() == Some(&format!("{key}-track")))
            .unwrap();
        let fill = elements
            .iter()
            .find(|e| e.key.as_deref() == Some(&format!("{key}-fill")))
            .unwrap();
        assert_eq!(
            track.label.as_deref(),
            Some(format!("{percent:.0}% complete").as_str())
        );
        assert!(
            (fill.rect[2] - track.rect[2] * percent / 100.0).abs() < 0.1,
            "{key} must match the displayed percentage"
        );
    }
}

#[test]
fn icon_only_actions_have_action_names_not_glyph_labels() {
    let mut rt = rune::Runtime::new(init(), view, (880.0, 600.0), rune::Env::default());
    rt.frame(0.0);
    let refresh = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("page-refresh"))
        .unwrap();
    assert_eq!(refresh.label.as_deref(), Some("Refresh current page"));
    assert!(refresh.focusable);
}

#[test]
fn addon_artwork_uses_installed_manifests_and_curated_available_logos() {
    let mut s = init();
    s.navigation.page = Page::Addons;
    assert_eq!(
        visible_artwork(&s),
        vec![
            "https://i.imgur.com/7N6XGoO.png",
            "https://torrentio.strem.fun/images/logo_v1.png"
        ]
    );
    s.addons.installed = vec![serde_json::from_value(serde_json::json!({
        "manifest_url":"https://example.com/manifest.json",
        "disabled":true,
        "manifest":{"id":"com.stremio.torrentio.addon", "name":"Configured Torrentio", "logo":"https://example.com/custom.png"}
    })).unwrap()];
    assert_eq!(
        visible_artwork(&s),
        vec![
            "https://example.com/custom.png",
            "https://i.imgur.com/7N6XGoO.png"
        ]
    );
    s.addons.installed.push(s.addons.installed[0].clone());
    assert_eq!(
        visible_artwork(&s),
        vec![
            "https://example.com/custom.png",
            "https://i.imgur.com/7N6XGoO.png"
        ]
    );
    s.navigation.page = Page::Discover;
    assert!(visible_artwork(&s).is_empty());
}

#[test]
fn addon_cards_show_cached_logos_and_keep_fallbacks_and_status_separate() {
    let mut s = init();
    s.navigation.page = Page::Addons;
    s.addons.installed = vec![
        serde_json::from_value(serde_json::json!({
            "manifest_url":"https://example.com/manifest.json", "disabled":true,
            "manifest":{"id":"example", "name":"Example", "logo":"https://example.com/logo.png"}
        }))
        .unwrap(),
    ];
    let mut rt = rune::Runtime::new(s, view, (1000.0, 800.0), rune::Env::default());
    rt.frame(0.0);
    assert!(rt.find("addon-logo-fallback-0").is_some());
    let image = rune::Pixels::new();
    image.set(4, 2, vec![128; 32]);
    rt.update(|s, _| {
        s.artwork.posters = Some(crate::artwork::Artwork::player_fixture(&[
            ("https://example.com/logo.png", image.clone()),
            ("https://torrentio.strem.fun/images/logo_v1.png", image),
        ]));
    });
    rt.frame(0.0);
    assert!(rt.find("addon-logo-0").is_some());
    assert!(
        rt.find("addon-logo-fallback-1").is_some(),
        "Cinemeta has no logo"
    );
    assert!(
        rt.find("addon-logo-3").is_some(),
        "available Torrentio uses its curated logo"
    );
    assert!(
        rt.find("addon-logo-fallback-2").is_some(),
        "uncached Kitsu keeps its fallback"
    );
    let elements = rt.elements();
    let logo = elements
        .iter()
        .find(|e| e.key.as_deref() == Some("addon-logo-0"))
        .unwrap();
    assert_eq!(logo.label.as_deref(), Some("Example logo"));
    assert!(
        elements
            .iter()
            .any(|e| e.text.as_deref() == Some("Disabled"))
    );
    rt.update(|s, _| {
        s.addons.installed[0].manifest.logo = Some("https://example.com/broken.png".into());
    });
    rt.frame(0.0);
    assert!(
        rt.find("addon-logo-fallback-0").is_some(),
        "a cache miss or failed image keeps the fallback"
    );
}

#[test]
fn addon_artwork_tracks_visible_tiles_while_scrolling() {
    let mut s = init();
    s.navigation.page = Page::Addons;
    s.addons.installed = (0..32).map(|index| serde_json::from_value(serde_json::json!({
        "manifest_url":format!("https://example.com/{index}/manifest.json"),
        "manifest":{"id":format!("addon-{index}"), "name":format!("Addon {index}"), "logo":format!("https://example.com/{index}.png")}
    })).unwrap()).collect();
    assert_eq!(visible_artwork(&s).len(), 32);
    let mut rt = rune::Runtime::new(s, view, (1000.0, 800.0), rune::Env::default());
    rt.poll = Some(poll);
    for _ in 0..3 {
        rt.frame(0.0);
    }
    assert!(visible_artwork(&rt.state).contains(&"https://example.com/0.png".into()));
    assert!(
        visible_artwork(&rt.state)
            .contains(&"https://torrentio.strem.fun/images/logo_v1.png".into())
    );
    assert!(!visible_artwork(&rt.state).contains(&"https://example.com/31.png".into()));
    let viewport = rect(&rt, "addons-scroll");
    rt.wheel(viewport[0] + 20.0, viewport[1] + 20.0, -100_000.0);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let urls = visible_artwork(&rt.state);
    assert!(urls.contains(&"https://example.com/31.png".into()));
    assert!(!urls.contains(&"https://torrentio.strem.fun/images/logo_v1.png".into()));
    assert!(!urls.contains(&"https://example.com/0.png".into()));
    assert!(urls.len() < 32);
    rt.update(|s, _| s.navigation.page = Page::Discover);
    rt.frame(0.0);
    assert!(rt.state.artwork.poster_urls.is_empty());
}

#[test]
fn addon_tiles_keep_four_columns_and_stay_put_during_installation() {
    for width in [880.0, 1280.0, 1920.0] {
        let (mut s, tx) = state_with_events();
        let (services, requests) = crate::services::Services::install_fixture().unwrap();
        s.runtime.services = Some(services);
        super::super::scenes::scene_addons(&mut s);
        let custom = s.addons.installed.clone();
        let mut rt = rune::Runtime::new(s, view, (width, 820.0), rune::Env::default());
        rt.poll = Some(poll);
        rt.frame(0.0);
        let slots = [
            "addon-slot-0",
            "addon-slot-1",
            "addon-slot-2",
            "installed-addon-0",
        ];
        let before = slots.map(|key| rect(&rt, key));
        for bounds in before {
            assert!((bounds[1] - before[0][1]).abs() < 1.0);
            assert!((bounds[2] - before[0][2]).abs() < 1.0);
        }
        assert!(
            before
                .windows(2)
                .all(|pair| pair[0][0] + pair[0][2] <= pair[1][0])
        );
        let button = rect(&rt, "supported-addon-0");
        tap(&mut rt, "supported-addon-0");
        assert_eq!(requests.try_recv().unwrap(), crate::services::CINEMETA);
        assert_eq!(rect(&rt, "supported-addon-0"), button);
        assert!(rt.find("supported-addon-0-spinner").is_some());
        assert!(rt.find("supported-addon-1-spinner").is_none());
        for (key, bounds) in slots.into_iter().zip(before) {
            assert_eq!(rect(&rt, key), bounds);
        }
        let mut installed = custom;
        installed.push(
            serde_json::from_value(serde_json::json!({
                "manifest_url": crate::services::CINEMETA,
                "manifest": {"id":"com.linvo.cinemeta", "name":"Cinemeta"}
            }))
            .unwrap(),
        );
        tx.try_send(Event::Addons(Ok(installed))).unwrap();
        rt.frame(1.0 / 60.0);
        assert!(rt.state.addons.pending.is_none());
        assert!(rt.find("supported-addon-0").is_none());
        assert!(rt.find("toggle-addon-1").is_some());
        for (key, bounds) in slots.into_iter().zip(before) {
            assert_eq!(rect(&rt, key), bounds);
        }
    }
}

#[test]
fn addon_dialog_keeps_its_size_and_footer_position_in_every_install_state() {
    let mut s = init();
    s.navigation.page = Page::Addons;
    s.addons.install_open = true;
    s.addons.busy = false;
    let mut rt = rune::Runtime::new(s, view, (880.0, 600.0), rune::Env::default());
    rt.frame(0.0);
    let panel = rect(&rt, "addon-install-dialog");
    let button = rect(&rt, "install-addon");
    let input = rect(&rt, "manifest-frame");
    for (busy, error, complete) in [
        (true, None, false),
        (false, Some("Cannot fetch manifest"), false),
        (false, None, true),
    ] {
        rt.update(|s, _| {
            s.addons.busy = busy;
            s.addons.error = error.map(str::to_owned);
            s.addons.install_complete = complete;
        });
        rt.frame(0.0);
        assert_eq!(rect(&rt, "addon-install-dialog"), panel);
        assert_eq!(rect(&rt, "manifest-frame"), input);
        assert_eq!(
            rect(
                &rt,
                if complete {
                    "addon-install-done"
                } else {
                    "install-addon"
                }
            ),
            button
        );
        assert_eq!(rt.find("install-addon-spinner").is_some(), busy);
    }
}

#[test]
fn built_in_addons_install_immediately_without_opening_the_url_dialog() {
    for index in 0..3 {
        let mut s = init();
        let (services, requests) = crate::services::Services::install_fixture().unwrap();
        s.runtime.services = Some(services);
        s.navigation.page = Page::Addons;
        s.addons.busy = false;
        s.addons.install = TextEdit::new("https://example.invalid/custom/manifest.json");
        s.addons.error = Some("Previous failure".into());
        let mut rt = rune::Runtime::new(s, view, (1000.0, 1000.0), rune::Env::default());
        rt.frame(0.0);
        tap(&mut rt, &format!("supported-addon-{index}"));
        assert_eq!(
            requests.try_recv().unwrap(),
            [
                crate::services::CINEMETA,
                crate::services::KITSU,
                "https://torrentio.strem.fun/manifest.json"
            ][index]
        );
        assert!(
            requests.try_recv().is_err(),
            "One click dispatches one install"
        );
        assert!(rt.state.addons.busy, "one click must start installation");
        assert!(!rt.state.addons.install_open);
        assert!(rt.state.addons.error.is_none());
        assert_eq!(
            rt.state.addons.install.text,
            "https://example.invalid/custom/manifest.json"
        );
        assert!(
            !rt.elements()
                .iter()
                .find(|element| element.key.as_deref() == Some("add-addon"))
                .unwrap()
                .focusable
        );
    }
}

#[test]
fn custom_addons_still_require_the_url_dialog_and_explicit_install() {
    let mut s = init();
    let (services, requests) = crate::services::Services::install_fixture().unwrap();
    s.runtime.services = Some(services);
    s.navigation.page = Page::Addons;
    s.addons.busy = false;
    let mut rt = rune::Runtime::new(s, view, (1000.0, 800.0), rune::Env::default());
    rt.frame(0.0);
    tap(&mut rt, "add-addon");
    assert!(rt.state.addons.install_open);
    assert!(rt.find("addon-install-dialog").is_some());
    assert!(!rt.state.addons.busy);
    assert!(
        requests.try_recv().is_err(),
        "Opening the dialog does not install"
    );
    rt.update(|s, _| {
        s.addons.install = TextEdit::new("https://example.invalid/custom/manifest.json");
    });
    tap(&mut rt, "install-addon");
    assert_eq!(
        requests.try_recv().unwrap(),
        "https://example.invalid/custom/manifest.json"
    );
    assert!(
        requests.try_recv().is_err(),
        "One submission dispatches one install"
    );
    assert!(rt.state.addons.busy);
    assert!(rt.state.addons.install_open);
}

#[test]
fn built_in_install_failure_allows_retry_and_success_updates_the_list_without_a_dialog() {
    let (mut s, tx) = state_with_events();
    let (services, requests) = crate::services::Services::install_fixture().unwrap();
    s.runtime.services = Some(services);
    s.navigation.page = Page::Addons;
    s.addons.busy = false;
    let mut rt = rune::Runtime::new(s, view, (1000.0, 800.0), rune::Env::default());
    rt.poll = Some(poll);
    rt.frame(0.0);
    tap(&mut rt, "supported-addon-0");
    assert_eq!(requests.try_recv().unwrap(), crate::services::CINEMETA);
    tx.try_send(Event::Addons(Err("Cannot fetch manifest".into())))
        .unwrap();
    rt.frame(1.0 / 60.0);
    assert!(!rt.state.addons.busy);
    assert!(!rt.state.addons.install_open);
    assert_eq!(
        rt.state.addons.error.as_deref(),
        Some("Cannot fetch manifest")
    );
    tap(&mut rt, "supported-addon-0");
    assert_eq!(requests.try_recv().unwrap(), crate::services::CINEMETA);
    assert!(
        requests.try_recv().is_err(),
        "Only the original request and retry dispatch"
    );
    assert!(rt.state.addons.busy);
    assert!(rt.state.addons.error.is_none());
    let addon = serde_json::from_value(serde_json::json!({
        "manifest_url":crate::services::CINEMETA,
        "manifest":{"id":"com.linvo.cinemeta","name":"Cinemeta"}
    }))
    .unwrap();
    tx.try_send(Event::Addons(Ok(vec![addon]))).unwrap();
    rt.frame(1.0 / 60.0);
    assert!(!rt.state.addons.busy);
    assert!(!rt.state.addons.install_open);
    assert!(rt.find("installed-addon-0").is_some());
}

#[test]
fn unified_addons_page_and_busy_install_spinner() {
    let mut s = init();
    s.navigation.page = Page::Addons;
    let mut rt = rune::Runtime::new(s, view, (1000.0, 800.0), rune::Env::default());
    rt.frame(0.0);
    assert!(rt.find("supported-addon-0").is_some());
    assert!(rt.find("supported-addon-1").is_some());
    rt.update(|s, _| {
        s.addons.install_open = true;
        s.addons.busy = true;
    });
    rt.frame(0.0);
    let install = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("install-addon"))
        .unwrap();
    assert!(
        !install.focusable,
        "busy installation must not offer an active control"
    );
    assert_eq!(install.label.as_deref(), Some("Install, busy"));
    assert!(rt.find("install-addon-spinner").is_some());
}

#[test]
fn installed_addons_share_the_available_list_without_duplicate_cards() {
    let mut s = init();
    s.addons.busy = false;
    s.navigation.page = Page::Addons;
    s.addons.installed = vec![
        serde_json::from_value(serde_json::json!({
            "manifest_url": crate::services::CINEMETA,
        "manifest": {"id":"com.linvo.cinemeta", "name":"Cinemeta", "catalogs":[{"id":"top", "type":"movie"}]}
        }))
        .unwrap(),
    ];
    let mut rt = rune::Runtime::new(s, view, (1000.0, 800.0), rune::Env::default());
    rt.frame(0.0);
    assert!(rt.find("installed-addon-0").is_some());
    assert!(rt.find("supported-addon-1").is_some());
    let toggle = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("toggle-addon-0"))
        .unwrap();
    assert_eq!(toggle.label.as_deref(), Some("Disable"));
    let bounds = rect(&rt, "toggle-addon-0");
    let remove = rect(&rt, "remove-0");
    let card = rect(&rt, "installed-addon-0");
    assert_eq!(bounds[2], remove[2]);
    assert_eq!(bounds[3], remove[3]);
    assert_eq!(bounds[1], remove[1]);
    assert!(bounds[0] > card[0] + card[2] / 2.0);
    assert!(
        (remove[0] + remove[2] - (card[0] + card[2] - tokens::layout::PANEL_INSET)).abs() < 2.0
    );
    assert_eq!(rt.state.catalogs().len(), 1);
    rt.update(|s, _| s.addons.installed[0].disabled = true);
    rt.frame(0.0);
    let toggle = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("toggle-addon-0"))
        .unwrap();
    assert_eq!(toggle.label.as_deref(), Some("Enable"));
    assert_eq!(rect(&rt, "toggle-addon-0"), bounds);
    assert!(rt.state.catalogs().is_empty());
}

#[test]
fn install_completion_replaces_the_spinner_with_a_checkmark() {
    let (mut s, tx) = state_with_events();
    s.navigation.page = Page::Addons;
    s.addons.install_open = true;
    s.addons.busy = true;
    let mut rt = rune::Runtime::new(s, view, (1000.0, 800.0), rune::Env::default());
    rt.poll = Some(poll);
    rt.frame(0.0);
    assert!(rt.find("install-addon-spinner").is_some());
    tx.try_send(Event::Addons(Ok(vec![]))).unwrap();
    rt.frame(1.0 / 60.0);
    assert!(rt.find("addon-install-success").is_some());
    tap(&mut rt, "addon-install-done");
    assert!(!rt.state.addons.install_open);
}

#[test]
fn failed_install_stops_the_spinner_and_restores_retry() {
    let (mut s, tx) = state_with_events();
    s.addons.install_open = true;
    s.addons.busy = true;
    let mut rt = rune::Runtime::new(s, view, (1000.0, 800.0), rune::Env::default());
    rt.poll = Some(poll);
    rt.frame(0.0);
    tx.try_send(Event::Addons(Err("Cannot fetch manifest".into())))
        .unwrap();
    rt.frame(1.0 / 60.0);
    assert!(rt.state.addons.install_open);
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.text.as_deref() == Some("Cannot fetch manifest"))
    );
    assert!(
        rt.elements()
            .iter()
            .find(|e| e.key.as_deref() == Some("install-addon"))
            .unwrap()
            .focusable
    );
}

#[test]
fn button_states_gate_actions_and_share_selection_styling() {
    struct Presses {
        count: u32,
    }
    fn controls(_: &Presses, _: &mut rune::Cx<Presses>) -> rune::El {
        rune::col()
            .child(
                Button::new("enabled", "Choose")
                    .selected(true)
                    .on_press(|n: &mut Presses| n.count += 1),
            )
            .child(
                Button::new("disabled", "Unavailable")
                    .enabled(false)
                    .on_press(|n: &mut Presses| n.count += 10),
            )
            .child(
                Button::new("busy", "Saving")
                    .busy(true)
                    .on_press(|n: &mut Presses| n.count += 100),
            )
    }
    let mut rt = rune::Runtime::new(
        Presses { count: 0 },
        controls,
        (400.0, 300.0),
        rune::Env::default(),
    );
    rt.frame(0.0);
    let selected = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("enabled"))
        .unwrap();
    assert_eq!(selected.fill, tokens::selected());
    assert_eq!(selected.label.as_deref(), Some("Choose, selected"));
    for name in ["disabled", "busy"] {
        let (x, y) = rt.find(name).unwrap();
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        assert_eq!(rt.state.count, 0, "{name} must not run its callback");
        assert!(
            !rt.elements()
                .into_iter()
                .find(|e| e.key.as_deref() == Some(name))
                .unwrap()
                .focusable
        );
    }
    rt.key(rune::KeyEvent {
        key: rune::Key::Tab,
        mods: rune::Mods::default(),
    });
    rt.frame(0.0);
    assert!(
        rt.elements()
            .into_iter()
            .any(|e| e.key.as_deref() == Some("enabled") && e.focused)
    );
    rt.key(rune::KeyEvent {
        key: rune::Key::Text(" ".into()),
        mods: rune::Mods::default(),
    });
    rt.frame(0.0);
    assert_eq!(
        rt.state.count, 1,
        "Space activation must reach the supplied callback"
    );
}

#[test]
fn input_submission_uses_the_callers_callback_not_its_key() {
    struct Form {
        edit: TextEdit,
        submitted: String,
    }
    fn form(s: &Form, _: &mut rune::Cx<Form>) -> rune::El {
        input(
            "arbitrary-field",
            &s.edit,
            "Value",
            |s: &mut Form| &mut s.edit,
            |_, _, _| false,
            |s| s.submitted.clone_from(&s.edit.text),
        )
    }
    let mut rt = rune::Runtime::new(
        Form {
            edit: TextEdit::new("A value"),
            submitted: String::new(),
        },
        form,
        (400.0, 300.0),
        rune::Env::default(),
    );
    rt.frame(0.0);
    let (x, y) = rt.find("arbitrary-field").unwrap();
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.key(rune::KeyEvent {
        key: rune::Key::Enter,
        mods: rune::Mods::default(),
    });
    rt.frame(0.0);
    assert_eq!(rt.state.submitted, "A value");
}
