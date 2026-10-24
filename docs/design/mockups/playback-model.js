// Design-preview model. Mirrors Stremio's watched and credits thresholds,
// not its full persistence, notifications, or player implementation.
export const WATCHED_THRESHOLD = 0.7;
export const CREDITS_THRESHOLD = 0.9;

export function updatePlayback(state, { position, duration, elapsed = 0, seek = false }) {
  if (!Number.isFinite(duration) || duration <= 0) return { ...state };
  const watchedTime = (state.watchedTime || 0) + (seek ? 0 : Math.max(0, elapsed));
  return {
    ...state,
    position: Math.max(0, Math.min(position, duration)),
    duration,
    watchedTime,
    watched: state.watched || watchedTime > duration * WATCHED_THRESHOLD,
  };
}

export function reconcilePlayback(state, episodes = [], now = Date.now()) {
  if (!(state.duration > 0 && state.position > state.duration * CREDITS_THRESHOLD)) return { ...state };
  const index = episodes.findIndex(episode => episode.id === state.videoId);
  const next = index < 0 ? undefined : episodes[index + 1];
  if (next && new Date(next.released).getTime() <= now) {
    return { ...state, videoId: next.id, position: 0, watchedTime: 0, watched: false, active: true };
  }
  return { ...state, position: 0, active: false };
}
