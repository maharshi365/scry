# Scry HTML screen preview

```powershell
node docs/design/mockups/server.mjs
```

Open http://127.0.0.1:4173. If that port is occupied:

```powershell
$env:PORT='4175'
node docs/design/mockups/server.mjs
```

The current review server uses http://127.0.0.1:4175.
Edits to HTML, CSS, or JavaScript reload the page. No dependencies or build step.
The server listens on loopback only.

## Screens

- Currently watching (start screen): movie progress and next series episodes.
- Movies and TV series: plain provider catalogs and filters.
- Library: To watch and Watched; type filter defaults to Movies.
- Calendar: seven-day release schedule with week navigation.
- Add-ons: Supported and Installed lists. All installations use one dialog.
- Title details, episode selection, sources, search, and player controls.

Use the Screen and State selectors in the sidebar to review all screens and
loading, empty, and error states. Right-click the page for Refresh. Right-click
a title or use its ⋯ button for watch-later and watched/unwatched actions.
Episode menus change the selected episode; series-level menus label their scope.
There is no Discover screen or playlist feature in this preview.

## Scope

This is an HTML prototype, not a change to the native Rust app. All actions use
in-memory sample data, which resets on reload. Refresh redraws the sample data;
it does not contact providers. Add-on forms do not fetch or store URLs. Playback
is simulated. Calendar entries are sample release dates, not an actual schedule.
Do not enter real credentials in the preview.

Colours follow the native shell and component definitions in
`crates/scry-app/src/ui/shell.rs` and `components.rs`. No external fonts are used.
The sample posters in `assets/` came from Metahub's public image service for
design review. They are not production app assets.

## Completion model

`playback-model.js` models Stremio's separate watched and credits thresholds:

- More than 70% of a video's duration accumulated as time watched marks it watched.
- Seeking changes position, not accumulated time watched.
- Leaving beyond 90% clears movie resume state or advances to the next released
  episode. At the last released episode, the title leaves Currently watching.
- Episode flags remain separate from a whole-series manual watched action.

Use the player's labelled preview controls to simulate watching or finishing.
The model is not a port of Stremio's complete player/library logic. It does not
implement account sync, notification reconciliation, automatic calendar refresh,
or actual playback. Adding the proposed behaviour to Rust is a separate task.

Sources checked on 24 October 2026, Stremio development commit
`029301839c863f2c7e5b7dcb5ca170d85280eda7`:

- [Threshold constants](https://github.com/Stremio/stremio-core/blob/029301839c863f2c7e5b7dcb5ca170d85280eda7/src/constants.rs)
- [Player state updates](https://github.com/Stremio/stremio-core/blob/029301839c863f2c7e5b7dcb5ca170d85280eda7/src/models/player.rs)

Run the preview model tests:

```powershell
node --test docs/design/mockups/playback-model.test.js
```
