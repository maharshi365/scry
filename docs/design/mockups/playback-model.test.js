import { test } from 'node:test';
import assert from 'node:assert/strict';
import { updatePlayback, reconcilePlayback } from './playback-model.js';

test('watched state uses time watched, not the seek position', () => {
  const result = updatePlayback({}, { position: 95, duration: 100, elapsed: 95, seek: true });
  assert.equal(result.watched, false);
  assert.equal(result.watchedTime, 0);
  assert.equal(result.position, 95);
});

test('more than 70 percent watched marks the video watched', () => {
  const boundary = updatePlayback({}, { position: 70, duration: 100, elapsed: 70 });
  assert.equal(boundary.watched, false);
  const completed = updatePlayback(boundary, { position: 71, duration: 100, elapsed: 1 });
  assert.equal(completed.watched, true);
});

test('unknown duration does not mark live media as watched', () => {
  assert.deepEqual(updatePlayback({ position: 5 }, { position: 10, duration: 0, elapsed: 200 }), { position: 5 });
});

const episodes = [{ id: 'e1', released: '2026-01-01' }, { id: 'e2', released: '2026-01-02' }];
test('leaving beyond 90 percent advances a series to the next released episode', () => {
  const result = reconcilePlayback({ videoId: 'e1', position: 91, duration: 100, watched: true }, episodes, Date.parse('2026-02-01'));
  assert.equal(result.videoId, 'e2');
  assert.equal(result.position, 0);
  assert.equal(result.watched, false);
  assert.equal(result.active, true);
});
test('90 percent exactly does not advance the episode', () => {
  assert.equal(reconcilePlayback({ videoId: 'e1', position: 90, duration: 100 }, episodes).videoId, 'e1');
});
test('finished movies and final episodes leave currently watching', () => {
  assert.equal(reconcilePlayback({ position: 91, duration: 100 }).active, false);
  assert.equal(reconcilePlayback({ videoId: 'e2', position: 91, duration: 100 }, episodes).active, false);
});
test('unreleased next episodes do not appear as playable', () => {
  const result = reconcilePlayback({ videoId: 'e1', position: 95, duration: 100 }, episodes, Date.parse('2026-01-01T12:00:00Z'));
  assert.equal(result.active, false);
  assert.equal(result.videoId, 'e1');
});
