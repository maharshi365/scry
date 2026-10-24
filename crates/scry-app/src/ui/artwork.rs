//! Visible poster selection and artwork presentation.
use super::{Page, State, components::accent};
use rune::{El, el, text, tok::TextStyle};
use scry_addons::Meta;

pub(super) const POSTER_CARD_HEIGHT: f32 = 302.0;
pub(super) const POSTER_GAP: f32 = 18.0;

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "The floored, nonnegative layout column count intentionally saturates to usize"
)]
pub(super) fn poster_columns(width: f32) -> usize {
    ((width + POSTER_GAP) / 174.0).floor().max(1.0) as usize
}

impl State {
    pub(super) fn refresh_artwork(&mut self) {
        let urls = visible_artwork(self);
        if urls != self.artwork_urls {
            if let Some(artwork) = &mut self.artwork {
                artwork.set_visible(&urls);
            }
            self.artwork_urls = urls;
        }
    }
}

pub(super) fn visible_artwork(s: &State) -> Vec<String> {
    let mut urls = Vec::new();
    if s.page == Page::Discover {
        for (index, meta) in s.metas.iter().enumerate() {
            let visible = if s.poster_route == s.route_key && !s.poster_rects.is_empty() {
                s.poster_viewport
                    .zip(s.poster_rects.get(&(index / s.poster_columns)))
                    .is_some_and(|(viewport, rect)| {
                        rect[1] < viewport[1] + viewport[3] && rect[1] + rect[3] > viewport[1]
                    })
            } else {
                index < 12
            };
            if visible
                && let Some(url) = &meta.poster
                && !urls.contains(url)
            {
                urls.push(url.clone());
            }
            if urls.len() == 32 {
                break;
            }
        }
    } else if s.page == Page::Detail
        && let Some(url) = s.detail.as_ref().and_then(|meta| meta.poster.as_ref())
    {
        urls.push(url.clone());
    }
    urls
}

pub(super) fn poster(s: &State, meta: &Meta, index: usize) -> El {
    let surface = el().w_full().h(246.0).radius(4.0).fill(rune::oklch(
        0.25 + [0.0, 0.025, 0.05][index % 3],
        0.035,
        230.0,
    ));
    if let Some(image) = meta
        .poster
        .as_ref()
        .and_then(|url| s.artwork.as_ref()?.get(url))
    {
        surface.pixels_fit(&image, rune::SurfaceFit::Fill)
    } else {
        surface.center().child(
            text(meta.name.chars().next().unwrap_or('S').to_string())
                .font(TextStyle::new(52.0, 300.0))
                .color(accent()),
        )
    }
}
