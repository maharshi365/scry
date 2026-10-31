use super::{Channel, ChannelSource, Collection, GuideAction, MAX_CHANNELS, Snapshot, Stored};
use crate::{
    Library, Media, MediaKind,
    catalog::media::{MediaRecord, hydrate, media_id, upsert_media},
    storage::schema::{media, playlist_items, playlists},
};
use anyhow::{Context, Result, ensure};
use diesel::{
    prelude::*,
    sql_types::{BigInt, Text},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GuideMetadata {
    pub released_at: Option<i64>,
    pub genres: Vec<String>,
    pub description: String,
    pub artwork: Option<String>,
}

#[derive(QueryableByName)]
struct Payload {
    #[diesel(sql_type = Text)]
    payload: String,
}

pub(crate) fn validate(connection: &mut SqliteConnection) -> Result<()> {
    diesel::sql_query("SELECT id,payload FROM guide_state LIMIT 0").load::<Payload>(connection)?;
    diesel::sql_query("SELECT media_id,payload FROM guide_metadata LIMIT 0")
        .load::<Payload>(connection)?;
    Ok(())
}

impl Library {
    /// Save a bounded metadata projection, without any source URLs.
    /// # Errors
    /// Returns an error for missing media or a failed write.
    pub fn set_guide_metadata(&mut self, media: &Media, metadata: &GuideMetadata) -> Result<()> {
        ensure!(
            metadata.genres.len() <= 64 && metadata.description.len() <= 32_768,
            "guide metadata is too large"
        );
        let connection = &mut *self.connection.borrow_mut();
        connection.immediate_transaction::<_, anyhow::Error, _>(|connection| {
            upsert_media(connection, media)?;
            let id = media_id(connection, &media.key)?;
            diesel::sql_query("INSERT INTO guide_metadata(media_id,payload) VALUES (?,?) ON CONFLICT(media_id) DO UPDATE SET payload=excluded.payload")
                .bind::<BigInt,_>(id).bind::<Text,_>(serde_json::to_string(metadata)?).execute(connection)?;
            Ok(())
        })
    }

    /// Update collections or channels and extend their saved schedule in one transaction.
    /// # Errors
    /// Returns an error for invalid changes or failed database operations.
    pub fn guide_update(&mut self, action: GuideAction, now: i64) -> Result<Snapshot> {
        ensure!((0..=253_402_000_000).contains(&now), "invalid guide clock");
        self.connection
            .borrow_mut()
            .immediate_transaction(|connection| update(connection, action, now))
    }
}

fn collections(connection: &mut SqliteConnection) -> Result<Vec<Collection>> {
    let records = playlists::table
        .order(playlists::playlist_id)
        .limit(200)
        .load::<(i64, String)>(connection)?;
    records
        .into_iter()
        .map(|(id, name)| {
            let records = media::table
                .inner_join(playlist_items::table)
                .filter(playlist_items::playlist_id.eq(id))
                .order(playlist_items::position)
                .limit(200)
                .select(MediaRecord::as_select())
                .load(connection)?;
            Ok(Collection {
                playlist: crate::Playlist { id, name },
                members: hydrate(connection, records)?,
            })
        })
        .collect()
}

fn valid_name(name: &str) -> Result<&str> {
    let name = name.trim();
    ensure!(
        !name.is_empty() && name.chars().count() <= 80,
        "collection name must contain 1 to 80 characters"
    );
    Ok(name)
}

#[expect(
    clippy::too_many_lines,
    reason = "Each guide edit is validated and applied within the same storage transaction"
)]
fn update(connection: &mut SqliteConnection, action: GuideAction, now: i64) -> Result<Snapshot> {
    let mut stored: Stored = diesel::sql_query("SELECT payload FROM guide_state WHERE id=1")
        .get_result::<Payload>(connection)
        .optional()?
        .map(|row| serde_json::from_str(&row.payload))
        .transpose()?
        .unwrap_or_default();
    let mut groups = collections(connection)?;
    match action {
        GuideAction::Refresh => {}
        GuideAction::CreateCollection(name) => {
            ensure!(groups.len() < 64, "collection limit reached");
            diesel::insert_into(playlists::table)
                .values(playlists::name.eq(valid_name(&name)?))
                .execute(connection)?;
        }
        GuideAction::RenameCollection(id, name) => {
            ensure!(
                diesel::update(playlists::table.find(id))
                    .set(playlists::name.eq(valid_name(&name)?))
                    .execute(connection)?
                    == 1,
                "collection not found"
            );
            for channel in &mut stored.channels {
                if channel.source == ChannelSource::Collection(id) {
                    channel.name = valid_name(&name)?.into();
                }
            }
        }
        GuideAction::DeleteCollection(id) => {
            diesel::delete(playlists::table.find(id)).execute(connection)?;
            stored
                .channels
                .retain(|channel| channel.source != ChannelSource::Collection(id));
        }
        GuideAction::Membership {
            collection,
            media,
            included,
        } => {
            let group = groups
                .iter()
                .find(|group| group.playlist.id == collection)
                .context("collection not found")?;
            ensure!(
                matches!(media.key.kind, MediaKind::Series | MediaKind::Movie),
                "collections contain shows or movies, not episodes"
            );
            ensure!(
                group
                    .members
                    .iter()
                    .all(|member| member.key.kind == media.key.kind),
                "TV and movies need separate collections"
            );
            ensure!(
                stored
                    .channels
                    .iter()
                    .filter(|channel| channel.source == ChannelSource::Collection(collection))
                    .all(|channel| channel.kind == media.key.kind),
                "an enabled channel cannot change between TV and movies"
            );
            upsert_media(connection, &media)?;
            let id = media_id(connection, &media.key)?;
            if included {
                ensure!(group.members.len() < 200, "collection limit reached");
                let position = playlist_items::table
                    .filter(playlist_items::playlist_id.eq(collection))
                    .select(diesel::dsl::max(playlist_items::position))
                    .first::<Option<i64>>(connection)?
                    .unwrap_or(-1)
                    .checked_add(1)
                    .context("collection position overflow")?;
                diesel::insert_into(playlist_items::table)
                    .values((
                        playlist_items::playlist_id.eq(collection),
                        playlist_items::media_id.eq(id),
                        playlist_items::position.eq(position),
                    ))
                    .on_conflict_do_nothing()
                    .execute(connection)?;
            } else {
                diesel::delete(playlist_items::table.find((collection, id))).execute(connection)?;
            }
        }
        GuideAction::EnableCollection(id) => {
            let group = groups
                .iter()
                .find(|group| group.playlist.id == id)
                .context("collection not found")?;
            ensure!(
                !group.members.is_empty(),
                "add titles before enabling a channel"
            );
            let kind = group.members.first().context("empty collection")?.key.kind;
            ensure!(
                matches!(kind, MediaKind::Series | MediaKind::Movie)
                    && !diesel::select(diesel::dsl::exists(
                        media::table
                            .inner_join(playlist_items::table)
                            .filter(playlist_items::playlist_id.eq(id))
                            .filter(media::kind.ne(kind.as_str())),
                    ))
                    .get_result::<bool>(connection)?,
                "channels require a collection of only shows or only movies"
            );
            if !stored
                .channels
                .iter()
                .any(|c| c.source == ChannelSource::Collection(id))
            {
                ensure!(
                    stored.channels.len() < MAX_CHANNELS,
                    "channel limit reached"
                );
                stored.channels.push(Channel {
                    id: format!("collection-{id}"),
                    name: group.playlist.name.clone(),
                    kind,
                    source: ChannelSource::Collection(id),
                    slots: Vec::new(),
                });
            }
        }
        GuideAction::Policy(policy) => {
            ensure!(
                policy.provider.len() <= 200 && policy.quality.len() <= 40,
                "invalid source policy"
            );
            if stored.policy.order != policy.order
                || stored.policy.watched_only != policy.watched_only
            {
                // Keep the current broadcast in place, but apply shared rules
                // to upcoming programs rather than making users wait a day.
                for channel in &mut stored.channels {
                    channel.slots.retain(|slot| slot.starts_at <= now);
                }
            }
            stored.policy = policy;
        }
    }
    groups = collections(connection)?;
    super::scheduling::extend(
        connection,
        &mut stored.channels,
        &groups,
        &stored.policy,
        now,
    )?;
    diesel::sql_query("INSERT INTO guide_state(id,payload) VALUES(1,?) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload")
        .bind::<Text,_>(serde_json::to_string(&stored)?).execute(connection)?;
    Ok(Snapshot {
        channels: stored.channels,
        collections: groups,
        policy: stored.policy,
    })
}
