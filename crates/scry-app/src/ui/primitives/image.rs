use super::super::design::tokens;
use rune::{El, el};

#[derive(Clone, Copy)]
pub(in crate::ui) enum Variant {
    Poster,
    Thumbnail,
    Episode,
}

/// Shared image geometry. The caller supplies cached pixels or fallback content.
pub(in crate::ui) fn frame(title: &str, index: usize, variant: Variant) -> El {
    let surface = el().role("img").label(title).fill(tokens::poster(index));
    match variant {
        Variant::Poster => surface
            .w_full()
            .h(tokens::poster::HEIGHT)
            .radius(tokens::poster::RADIUS),
        Variant::Thumbnail => surface
            .w(tokens::poster::THUMBNAIL_WIDTH)
            .h(tokens::poster::THUMBNAIL_HEIGHT)
            .radius(tokens::poster::RADIUS),
        Variant::Episode => surface
            .w(tokens::poster::EPISODE_WIDTH)
            .h(tokens::poster::EPISODE_HEIGHT)
            .radius(tokens::radius::SM),
    }
}
