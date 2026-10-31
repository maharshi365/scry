// Diesel's schema types mirror schema.sql.
diesel::table! {
    media (media_id) {
        media_id -> BigInt,
        namespace -> Text,
        kind -> Text,
        external_id -> Text,
        title -> Text,
        parent_id -> Nullable<BigInt>,
    }
}
diesel::table! {
    watchlist (entry_id) {
        entry_id -> BigInt,
        media_id -> BigInt,
    }
}
diesel::table! {
    playlists (playlist_id) {
        playlist_id -> BigInt,
        name -> Text,
    }
}
diesel::table! {
    playlist_items (playlist_id, media_id) {
        playlist_id -> BigInt,
        media_id -> BigInt,
        position -> BigInt,
    }
}
diesel::table! {
    progress (media_id) {
        media_id -> BigInt,
        position_seconds -> Double,
        duration_seconds -> Double,
    }
}
diesel::table! {
    watched (media_id) {
        media_id -> BigInt,
        #[sql_name = "watched"]
        is_watched -> Bool,
    }
}
diesel::table! {
    effective_watched (media_id) {
        media_id -> BigInt,
        #[sql_name = "watched"]
        is_watched -> Bool,
    }
}
diesel::table! {
    series_inventory (media_id) {
        media_id -> BigInt,
    }
}
diesel::table! {
    viewing (media_id) {
        media_id -> BigInt,
        watched_seconds -> Double,
        active -> Bool,
        touched -> BigInt,
    }
}
diesel::table! {
    addons (id) {
        id -> Text,
        position -> BigInt,
        manifest_url -> Text,
        manifest_json -> Text,
    }
}
diesel::table! {
    episode_releases (media_id) {
        media_id -> BigInt,
        released_at -> Nullable<BigInt>,
        season -> Nullable<BigInt>,
        episode -> Nullable<BigInt>,
    }
}
diesel::joinable!(episode_releases -> media (media_id));
diesel::joinable!(watchlist -> media (media_id));
diesel::joinable!(playlist_items -> media (media_id));
diesel::joinable!(playlist_items -> playlists (playlist_id));
diesel::joinable!(progress -> media (media_id));
diesel::joinable!(watched -> media (media_id));
diesel::joinable!(effective_watched -> media (media_id));
diesel::joinable!(viewing -> media (media_id));
diesel::allow_tables_to_appear_in_same_query!(
    media,
    watchlist,
    playlists,
    playlist_items,
    progress,
    watched,
    effective_watched,
    series_inventory,
    viewing,
    addons,
    episode_releases
);
