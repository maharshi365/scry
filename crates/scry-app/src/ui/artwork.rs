//! Visible poster selection and artwork presentation.
use super::{
    Page, State,
    design::tokens::{self, accent, type_},
};
use rune::{El, el, icon, row, text};

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
        let urls = player_artwork(self);
        if urls != self.player_artwork_urls {
            if let Some(artwork) = &mut self.player_artwork {
                artwork.set_visible(&urls);
            }
            self.player_artwork_urls = urls;
        }
    }
}

pub(super) fn player_artwork(s: &State) -> Vec<String> {
    let (background, logo) =
        if s.navigation.page == Page::Player && s.player.playing && !s.player.ready {
            (s.player.background.as_ref(), s.player.logo.as_ref())
        } else if s.navigation.page == Page::Detail {
            s.detail.as_ref().map_or((None, None), |meta| {
                (meta.background.as_ref(), meta.logo.as_ref())
            })
        } else {
            (None, None)
        };
    background.into_iter().chain(logo).cloned().collect()
}

pub(super) fn visible_artwork(s: &State) -> Vec<String> {
    let mut urls = Vec::new();
    if matches!(
        s.navigation.page,
        Page::Discover | Page::Library | Page::Watching
    ) {
        return super::poster_grid::artwork_urls(s);
    } else if s.navigation.page == Page::Detail
        && let Some(url) = s.detail.as_ref().and_then(|meta| meta.poster.as_ref())
    {
        urls.push(url.clone());
    } else if s.navigation.page == Page::Calendar {
        let c = &s.calendar;
        if let Some(day) = c.expanded_day {
            for (index, release) in c.releases.iter().filter(|r| r.date == day).enumerate() {
                let visible = c.dialog_artwork_rects.get(&(day, index)).is_none_or(|r| {
                    c.dialog_viewport
                        .is_none_or(|viewport| intersects(*r, viewport))
                });
                if visible
                    && let Some(url) = &release.poster
                    && !urls.contains(url)
                {
                    urls.push(url.clone());
                }
                if urls.len() == 32 {
                    break;
                }
            }
            return urls;
        }
        let days = super::calendar::dates(c.anchor, c.mode);
        let mut indices = std::collections::BTreeMap::new();
        for release in &c.releases {
            if release.date < days[0] || release.date > days[days.len() - 1] {
                continue;
            }
            let index = indices.entry(release.date).or_insert(0usize);
            let key = format!("calendar-episode-{}-{index}", release.date);
            *index += 1;
            let visible = c.artwork_rects.get(&key).is_none_or(|r| {
                c.day_viewports
                    .get(&release.date)
                    .is_none_or(|day| intersects(*r, *day))
                    && c.viewport.is_none_or(|viewport| intersects(*r, viewport))
            });
            if visible
                && let Some(url) = &release.poster
                && !urls.contains(url)
            {
                urls.push(url.clone());
            }
            if urls.len() == 32 {
                break;
            }
        }
    }
    urls
}

pub(super) fn poster(s: &State, title: &str, url: Option<&str>, index: usize) -> El {
    let surface = el()
        .role("img")
        .label(title)
        .w_full()
        .h(246.0)
        .radius(4.0)
        .fill(tokens::poster(index));
    if let Some(image) = url.and_then(|url| s.artwork.as_ref()?.get(url)) {
        surface.pixels_fit(&image, rune::SurfaceFit::Fill)
    } else {
        surface.center().child(
            text(title.chars().next().unwrap_or('S').to_string())
                .key(format!("poster-fallback-{index}"))
                .font(type_::POSTER_FALLBACK)
                .color(accent()),
        )
    }
}

pub(super) fn poster_overlay(
    s: &State,
    surface: El,
    media: Option<&scry_library::MediaKey>,
    key: &str,
    height: f32,
) -> El {
    let surface = surface.key(key);
    let Some(media) = media else {
        return surface;
    };
    let (watched, progress) = if media.kind == scry_library::MediaKind::Series {
        s.series_status(media)
    } else {
        s.movie_status(media)
    };
    poster_status_overlay(surface, key, height, watched, progress)
}

pub(super) fn poster_status_overlay(
    surface: El,
    key: &str,
    height: f32,
    watched: bool,
    progress: Option<scry_library::Progress>,
) -> El {
    let surface = surface.key(key);
    if watched {
        return surface.child(
            row()
                .absolute(0.0, 8.0)
                .w_full()
                .h(24.0)
                .px(8.0)
                .child(el().grow(1.0))
                .child(
                    el().key(format!("{key}-watched"))
                        .role("img")
                        .label("Watched")
                        .w(24.0)
                        .h(24.0)
                        .radius(12.0)
                        .fill(tokens::scrim())
                        .center()
                        .child(icon(&super::design::icons::CHECK, 16.0).color(tokens::success())),
                ),
        );
    }
    let Some(progress) = progress.filter(|p| p.position_seconds > 0.0 && p.duration_seconds > 0.0)
    else {
        return surface;
    };
    #[expect(
        clippy::cast_possible_truncation,
        reason = "A clamped progress fraction fits f32"
    )]
    let fraction = (progress.position_seconds / progress.duration_seconds).clamp(0.0, 1.0) as f32;
    surface.child(
        row()
            .absolute(0.0, height * 5.0 / 6.0 - 3.0)
            .w_full()
            .h(6.0)
            .px(8.0)
            .child(
                row()
                    .key(format!("{key}-progress-track"))
                    .role("progressbar")
                    .label(format!("{:.0}% complete", fraction * 100.0))
                    .grow(1.0)
                    .h(6.0)
                    .radius(3.0)
                    .fill(tokens::scrim())
                    .child(
                        el().key(format!("{key}-progress-fill"))
                            .grow(fraction)
                            .h_full()
                            .radius(3.0)
                            .fill(accent()),
                    )
                    .child(el().grow(1.0 - fraction).h_full()),
            ),
    )
}

fn intersects(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[0] + b[2] && a[0] + a[2] > b[0] && a[1] < b[1] + b[3] && a[1] + a[3] > b[1]
}

pub(super) fn poster_thumbnail(s: &State, key: &str, url: Option<&str>, title: &str) -> El {
    let surface = el()
        .key(key)
        .role("img")
        .label(title)
        .w(32.0)
        .h(48.0)
        .radius(4.0)
        .fill(tokens::poster(0));
    if let Some(image) = url.and_then(|url| s.artwork.as_ref()?.get(url)) {
        surface.child(
            el().key(format!("{key}-image"))
                .w_full()
                .h_full()
                .pixels_fit(&image, rune::SurfaceFit::Fill),
        )
    } else {
        surface.center().child(
            text(title.chars().next().unwrap_or('S').to_string())
                .key(format!("{key}-fallback"))
                .font(type_::THUMBNAIL_FALLBACK)
                .color(accent()),
        )
    }
}
