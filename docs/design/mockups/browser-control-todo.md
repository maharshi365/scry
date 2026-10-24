# Browser review note

- Browser Control reports the bundled shim as 0.0.25; relay version not checked.
- Session: `mcp-3dc67099`; page: local HTML preview on port 4175.
- 24 October 2026: on the next execute after the prior review, the tool reported
  “The session default page was closed; created a new page.”
- Reproduction context: an earlier screenshot succeeded; the next call navigated
  to the local preview after the review tab had been closed.
- Expected: reuse an open review tab. Actual: a fresh session-owned tab opened.
- Recovery: navigated the new page to the preview and verified its Currently
  watching heading. No stale page or element references were reused.
- No unresolved preview blocker. No browser setup changes made.
- A later preview test timed out after 30 seconds while locating the button
  `Open Dune: Part Two`. Expected: the start-screen title button. Actual: no
  matching button on the current page. No click or outward mutation occurred.
  That browser check was not retried; review moved to native Rune tests and
  offscreen screenshots after implementation was approved.
