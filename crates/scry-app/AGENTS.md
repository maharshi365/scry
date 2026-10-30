# App UI rules

- Use the shared controls in `src/ui/primitives` where possible. Add reusable
  controls there, not inside a feature component.
- Keep primitives free of app state, services, navigation, and playback policy.
  Pass values and callbacks from feature modules.
- Define visual tokens in `src/ui/design/tokens.rs`. Reuse existing colors,
  typography, corner, border, opacity, and control-size tokens. Do not invent
  local alternatives. Keep shared icons in `src/ui/design/icons.rs`.
- Check for an existing token with the same purpose before adding one. Reuse
  tokens where their meaning fits, not just because their values match. Always
  ask for approval before adding a new token.
- Use shared dropdown triggers with explicit transparent or framed variants.
  Use the shared dropdown surface and flat menu rows for the open menu.
- Use typed, CVA-style variant axes for primitive appearance, size, and state.
  Resolve base styles, variants, and compound states inside the primitive.
  Extend that API when a screen needs a reusable variant; do not restyle a
  primitive with local fill, border, corner, or control-size overrides.
- Avoid cards inside cards. Dropdowns and player menus must have one outer
  surface, with unframed lists and sections inside. Use spacing or dividers
  instead of another bordered panel.
- Keep feature-specific layout and behavior in feature modules. Do not move
  business rules into visual primitives.
- Use `primitives/pattern.rs` for shared pages, detail headers, sections, and
  settings groups. Settings groups own separators and one outer surface.
- Keep theme values and shared layout/motion parameters in `design/tokens.rs`.
  Features choose semantic roles; primitives own their appearance. A theme edit
  must not require edits to feature screens.
- Reserve primary styling for at most one main action per screen or modal.
  Repeated row actions are secondary. Status is not an action: use
  `primitives/feedback.rs` for non-interactive status and recovery messages.
- Use `primitives/motion.rs` for motion intent. Hover feedback is immediate
  and belongs only to the active target; do not animate passive settings rows.
- Keep playground examples isolated from real services and data. Add a named
  fixture and interaction/layout coverage for new shared patterns; inspect
  rendered scenes before accepting visual changes.
- Extend UI behavior tests and the design-policy test when adding shared
  controls or visual rules. Follow the root `AGENTS.md` checks before handoff.
