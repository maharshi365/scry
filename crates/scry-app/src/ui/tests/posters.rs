use super::*;

fn gallery() -> rune::Runtime<State> {
    let mut state = init();
    scene_discover(&mut state);
    let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
    for _ in 0..4 {
        rt.frame(0.0);
    }
    rt
}

fn advance(rt: &mut rune::Runtime<State>) {
    for _ in 0..90 {
        rt.frame(1.0 / 60.0);
    }
}

#[test]
fn gallery_cards_are_unframed_and_reserve_two_title_lines() {
    let rt = gallery();
    let elements = rt.elements();
    let card = elements
        .iter()
        .find(|e| e.key.as_deref() == Some("title-0"))
        .unwrap();
    assert_eq!(
        card.fill.a, 0.0,
        "The poster must not sit inside a filled card"
    );
    let title = rect(&rt, "poster-title-title-0");
    let subtitle = rect(&rt, "title-info-0");
    assert!(
        title[3] >= 30.0,
        "Reserve room for a wrapped title: {title:?}"
    );
    assert!(subtitle[1] >= title[1] + title[3]);
    assert!(subtitle[1] + subtitle[3] <= card.rect[1] + card.rect[3] + 0.5);
}

#[test]
fn gallery_zoom_is_centered_clipped_and_does_not_follow_the_pointer() {
    let mut rt = gallery();
    let frame = rect(&rt, "title-poster-0");
    let title = rect(&rt, "poster-title-title-0");
    let neighbor = rect(&rt, "title-poster-1");
    let neighbor_artwork = rect(&rt, "title-poster-1-artwork");
    rt.pointer_move(frame[0] + frame[2] * 0.2, frame[1] + frame[3] * 0.2);
    advance(&mut rt);
    let zoomed = rect(&rt, "title-poster-0-artwork");
    assert!(
        zoomed[2] > frame[2] && zoomed[3] > frame[3],
        "Only the artwork zooms"
    );
    assert!(zoomed[2] <= frame[2] * 1.041, "The zoom must stay subtle");
    for axis in 0..2 {
        assert!(
            (zoomed[axis] + zoomed[axis + 2] / 2.0 - frame[axis] - frame[axis + 2] / 2.0).abs()
                < 0.5
        );
    }
    let artwork = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("title-poster-0-artwork"))
        .unwrap();
    assert!(artwork.visible[2] <= frame[2] + 0.5 && artwork.visible[3] <= frame[3] + 0.5);
    rt.pointer_move(frame[0] + frame[2] * 0.8, frame[1] + frame[3] * 0.8);
    advance(&mut rt);
    assert_eq!(rect(&rt, "title-poster-0-artwork"), zoomed);
    assert_eq!(rect(&rt, "title-poster-0"), frame);
    assert_eq!(rect(&rt, "poster-title-title-0"), title);
    assert_eq!(rect(&rt, "title-poster-1"), neighbor);
    assert_eq!(rect(&rt, "title-poster-1-artwork"), neighbor_artwork);
    rt.pointer_move(0.0, 599.0);
    advance(&mut rt);
    let resting = rect(&rt, "title-poster-0-artwork");
    assert!((resting[2] - frame[2]).abs() < 0.5 && (resting[3] - frame[3]).abs() < 0.5);
}

#[test]
fn gallery_does_not_zoom_with_reduced_motion() {
    let mut rt = gallery();
    let resting = rect(&rt, "title-poster-0-artwork");
    let frame = rect(&rt, "title-poster-0");
    rt.pointer_move(frame[0] + frame[2] / 2.0, frame[1] + frame[3] / 2.0);
    advance(&mut rt);
    assert!(rect(&rt, "title-poster-0-artwork")[2] > resting[2]);
    rt.set_system_reduced_motion(true);
    rt.frame(0.0);
    assert_eq!(rect(&rt, "title-poster-0-artwork"), resting);
    rt.pointer_move(0.0, 599.0);
    rt.frame(0.0);
    rt.pointer_move(frame[0] + frame[2] / 2.0, frame[1] + frame[3] / 2.0);
    advance(&mut rt);
    assert_eq!(rect(&rt, "title-poster-0-artwork"), resting);
}
