//! Visible poster selection and artwork presentation.
use super::{
    Page, State,
    design::tokens::{self, accent, type_},
    primitives::progress,
};
use rune::{El, el, row, text};

#[derive(Default)]
pub(super) struct ArtworkState {
    pub posters: Option<crate::artwork::Artwork>,
    pub poster_urls: Vec<String>,
    pub player: Option<crate::artwork::Artwork>,
    pub player_urls: Vec<String>,
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "The floored, nonnegative layout column count intentionally saturates to usize"
)]
pub(super) fn poster_columns(width: f32) -> usize {
    ((width + tokens::poster::GAP) / (tokens::poster::MIN_WIDTH + tokens::poster::GAP))
        .floor()
        .max(1.0) as usize
}

impl State {
    pub(super) fn refresh_artwork(&mut self) {
        let urls = visible_artwork(self);
        if urls != self.artwork.poster_urls {
            if let Some(artwork) = &mut self.artwork.posters {
                artwork.set_visible(&urls);
            }
            self.artwork.poster_urls = urls;
        }
        let urls = player_artwork(self);
        if urls != self.artwork.player_urls {
            if let Some(artwork) = &mut self.artwork.player {
                artwork.set_visible(&urls);
            }
            self.artwork.player_urls = urls;
        }
    }
}

pub(super) fn player_artwork(s: &State) -> Vec<String> {
    let (background, logo) =
        if s.navigation.page == Page::Player && s.player.playing && !s.player.ready {
            (s.player.background.as_ref(), s.player.logo.as_ref())
        } else if s.navigation.page == Page::Detail {
            s.detail.meta.as_ref().map_or((None, None), |meta| {
                (meta.background.as_ref(), meta.logo.as_ref())
            })
        } else {
            (None, None)
        };
    background.into_iter().chain(logo).cloned().collect()
}

pub(super) fn visible_artwork(s: &State) -> Vec<String> {
    let mut urls = Vec::new();
    if s.navigation.page == Page::Guide {
        return s.guide_artwork();
    }
    if s.navigation.page == Page::Addons {
        return super::addons::artwork_urls(s);
    } else if (s.navigation.page == Page::Detail && s.detail.preview)
        || matches!(
            s.navigation.page,
            Page::Discover | Page::Library | Page::Watching
        )
    {
        return super::poster_grid::artwork_urls(s);
    } else if s.navigation.page == Page::Detail {
        if let Some(url) = s.detail.meta.as_ref().and_then(|meta| meta.poster.as_ref()) {
            urls.push(url.clone());
        }
        if let Some(meta) = &s.detail.meta {
            for index in s.visible_episode_indices() {
                if let Some(url) = &meta.videos[index].thumbnail {
                    if urls.len() >= 32 {
                        break;
                    }
                    if !urls.contains(url) {
                        urls.push(url.clone());
                    }
                }
            }
        }
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
        let days = super::calendar::dates(c.anchor);
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
    let surface =
        super::primitives::image::frame(title, index, super::primitives::image::Variant::Poster);
    if let Some(image) = url.and_then(|url| s.artwork.posters.as_ref()?.get(url)) {
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
                .child(super::primitives::status::watched_badge(
                    format!("{key}-watched"),
                    24.0,
                )),
        );
    }
    let Some(fraction) = progress
        .and_then(scry_library::Progress::fraction)
        .filter(|fraction| *fraction > 0.0)
    else {
        return surface;
    };
    surface.child(
        row()
            .absolute(0.0, height * 7.0 / 8.0 - tokens::progress::HEIGHT / 2.0)
            .w_full()
            .h(tokens::progress::HEIGHT)
            .px(8.0)
            .child(progress::bar(&format!("{key}-progress"), fraction)),
    )
}

pub(super) fn intersects(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[0] + b[2] && a[0] + a[2] > b[0] && a[1] < b[1] + b[3] && a[1] + a[3] > b[1]
}

pub(super) fn poster_thumbnail(s: &State, key: &str, url: Option<&str>, title: &str) -> El {
    let surface =
        super::primitives::image::frame(title, 0, super::primitives::image::Variant::Thumbnail)
            .key(key);
    if let Some(image) = url.and_then(|url| s.artwork.posters.as_ref()?.get(url)) {
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
