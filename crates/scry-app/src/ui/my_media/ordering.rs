use super::super::{
    State,
    design::tokens,
    primitives::{
        button::Button,
        feedback::{self, Tone},
        pattern,
        text::label,
    },
};
use crate::{
    local_media::{EpisodeOrder, Ordering},
    services::my_media::Command,
};
use rune::{El, col, row};

#[derive(Default)]
pub(super) struct Review {
    generation: u64,
    choices: Vec<Ordering>,
}

impl Review {
    pub fn reset(&mut self) {
        self.generation += 1;
        self.choices.clear();
    }
}

impl State {
    fn find_media_orders(&mut self, mode: EpisodeOrder) {
        let Some(id) = self.my_media.selected.clone() else {
            return;
        };
        self.my_media.orders.reset();
        self.media_command(Command::FindOrders(
            self.my_media.orders.generation,
            id,
            mode,
        ));
    }

    pub(in crate::ui) fn apply_media_orders(
        &mut self,
        token: u64,
        id: &str,
        result: Result<Vec<Ordering>, String>,
    ) {
        self.my_media.busy = false;
        if token != self.my_media.orders.generation || self.my_media.selected.as_deref() != Some(id)
        {
            return;
        }
        match result {
            Ok(choices) => {
                self.my_media.orders.choices = choices;
                self.my_media.status =
                    "Select an episode order to apply it. The current mapping has not changed."
                        .into();
            }
            Err(error) => self.my_media.error = Some(error),
        }
    }
}

pub(super) fn view(s: &State) -> El {
    let media = &s.my_media;
    let Some(item) = media
        .index
        .items
        .iter()
        .find(|item| Some(&item.id) == media.selected.as_ref())
    else {
        return col();
    };
    if item
        .matched
        .as_ref()
        .is_none_or(|matched| matched.kind != "series")
    {
        return col();
    }
    let mut modes = row().w_full().flex_wrap().gap(tokens::layout::ACTION_GAP);
    for (key, mode) in [
        ("media-order-aired", EpisodeOrder::Aired),
        ("media-order-absolute", EpisodeOrder::Absolute),
        ("media-order-dvd", EpisodeOrder::Dvd),
    ] {
        modes = modes.child(
            Button::new(key, mode.label())
                .enabled(!media.busy)
                .selected(
                    item.ordering
                        .as_ref()
                        .is_some_and(|order| order.mode == mode),
                )
                .on_press(move |s: &mut State| s.find_media_orders(mode)),
        );
    }
    let current = item
        .ordering
        .as_ref()
        .map_or("Filename numbering (unverified)", |order| &order.name);
    let mut content = col().w_full().gap(tokens::layout::ACTION_GAP)
        .child(label(format!("Current order: {current}"))).child(modes)
        .child(label("Fetch an order, then select it below. Canonical provider IDs stay unchanged; per-file corrections take priority. Unresolved files remain playable under Other episodes."));
    if let Some(order) = &item.ordering
        && let Some(source) = &order.source
    {
        content = content.child(attribution(source));
    }
    for (index, choice) in media.orders.choices.iter().enumerate() {
        let choice = choice.clone();
        let id = item.id.clone();
        let mut candidate = col().w_full().gap(tokens::layout::COPY_GAP).child(
            Button::new(
                format!("media-order-choice-{index}"),
                format!(
                    "Apply {} · {} episode links",
                    choice.name,
                    choice.episodes.len()
                ),
            )
            .enabled(!media.busy)
            .on_press(move |s: &mut State| {
                s.media_command(Command::ApplyOrder(id.clone(), Some(choice.clone())));
            }),
        );
        if let Some(source) = &media.orders.choices[index].source {
            candidate = candidate.child(attribution(source));
        }
        content = content.child(candidate);
    }
    if item.ordering.is_some() {
        let id = item.id.clone();
        content = content.child(
            Button::new("media-order-reset", "Reset to filename numbering")
                .enabled(!media.busy)
                .on_press(move |s: &mut State| {
                    s.media_command(Command::ApplyOrder(id.clone(), None));
                }),
        );
    }
    pattern::section(
        "media-ordering",
        "Episode ordering",
        "Select aired, anime absolute, or a verified DVD release order for this show.",
        content,
    )
}

fn attribution(source: &str) -> El {
    let source = source.to_owned();
    col()
        .w_full()
        .child(feedback::status(
            "media-order-license",
            "DVD data: TVmaze · CC BY-SA 4.0",
            Tone::Neutral,
        ))
        .child(
            Button::new(
                format!("media-order-source-{source}"),
                "View order on TVmaze",
            )
            .on_press_tx(move |s: &mut State, tx| {
                let valid = url::Url::parse(&source).is_ok_and(|url| {
                    url.scheme() == "https"
                        && url.host_str() == Some("www.tvmaze.com")
                        && url.username().is_empty()
                        && url.password().is_none()
                });
                if !valid || tx.open_url(&source).is_err() {
                    s.my_media.error = Some("Cannot open the episode-order source".into());
                }
            }),
        )
}

pub(in crate::ui) fn scene(s: &mut State) {
    use crate::local_media::{Match, OrderedEpisode};
    super::scene(s);
    let Some(item) = s
        .my_media
        .index
        .items
        .iter_mut()
        .find(|item| item.kind == "series")
    else {
        return;
    };
    item.title = "Firefly".into();
    item.matched = Some(Match {
        id: "tt0303461".into(),
        kind: "series".into(),
        name: "Firefly".into(),
        year: "2002".into(),
        poster: None,
        manual: true,
    });
    item.ordering = Some(Ordering {
        series_id: "tt0303461".into(),
        mode: EpisodeOrder::Dvd,
        name: "Firefly DVD release".into(),
        source: Some("https://www.tvmaze.com/alternatelists/1".into()),
        episodes: vec![OrderedEpisode {
            file_season: 1,
            file_episode: 1,
            id: "tt0303461:1:11".into(),
            title: "Serenity".into(),
            season: 1,
            episode: 11,
            released: None,
        }],
    });
    let id = item.id.clone();
    s.review_media(&id);
}
