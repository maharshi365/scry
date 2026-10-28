# App UI rules

- Use the shared controls in `src/ui/primitives` where possible. Add reusable
  controls there, not inside a feature component.
- Keep primitives free of app state, services, navigation, and playback policy.
  Pass values and callbacks from feature modules.
- Define visual tokens in `src/ui/design/tokens.rs`. Reuse existing colors,
  typography, corner, border, opacity, and control-size tokens. Do not invent
  local alternatives. Keep shared icons in `src/ui/design/icons.rs`.
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
- Extend UI behavior tests and the design-policy test when adding shared
  controls or visual rules. Follow the root `AGENTS.md` checks before handoff.
