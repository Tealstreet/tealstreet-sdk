/**
 * Deterministic record/replay of the exchange worker's OUTBOUND message stream.
 *
 * Why this exists: front-end performance work on the trade page was
 * unmeasurable. Idle blocking on one production build measured 18.3%, 24.0%,
 * 25.5% and 26.6% across four runs — the spread came entirely from how fast
 * the venue happened to be pushing data that minute, and it was larger than
 * most optimisations are worth. Any A/B smaller than ~8 points was noise.
 *
 * The fix is to freeze the input. Every byte the trade page renders arrives as
 * a `postMessage` from this worker, so recording that one stream and replaying
 * it on a fixed schedule makes two runs comparable.
 *
 * **This is the only supported way to benchmark the trade page.** Do not go
 * back to wrapping `requestAnimationFrame` / `Worker` from the page to measure
 * things — that was tried, and it blanked the chart, orderbook, depth chart,
 * place-order and screener modules while still producing plausible-looking
 * numbers.
 *
 * Dev-only: the host app must opt in explicitly (see `perfFixture` on
 * `WorkerConfig`). With no config the mode is `off` and every hook here is a
 * predicate returning false, so production pays one boolean per outbound
 * message.
 */
import type { PerfFixture, PerfFixtureConfig, PerfFixtureFrame } from './StoreWorkerPayload';

type Mode = 'off' | 'record' | 'replay';

/** Hard cap so a forgotten recorder cannot exhaust worker memory. */
const MAX_FRAMES = 20_000;

let mode: Mode = 'off';
let frames: PerfFixtureFrame[] = [];
let recordStartedAt = 0;

let replayTimer: ReturnType<typeof setTimeout> | null = null;
let replayFixture: PerfFixture | null = null;
let replayIntervalMs = 50;
let replayLoop = true;
let replayCursor = 0;

export const configurePerfFixture = (cfg: PerfFixtureConfig | undefined): void => {
  const next: Mode = cfg?.mode ?? 'off';
  if (next === mode && next !== 'replay') return;

  stopReplay();
  mode = next;

  if (mode === 'record') {
    frames = [];
    recordStartedAt = 0;
  }

  if (mode === 'replay') {
    replayFixture = cfg?.fixture ?? null;
    replayIntervalMs = Math.max(1, cfg?.intervalMs ?? 50);
    replayLoop = cfg?.loop !== false;
    replayCursor = 0;
  }
};

export const isRecordingPerfFixture = (): boolean => mode === 'record';

/**
 * True when the worker must NOT open real venue connections.
 *
 * Replay drives the main thread from the fixture alone; a live exchange
 * alongside it would put the very variance we are removing back into the
 * measurement.
 */
export const isReplayingPerfFixture = (): boolean => mode === 'replay';

export const recordOutboundFrame = (msg: any): void => {
  if (mode !== 'record') return;
  if (frames.length >= MAX_FRAMES) return;

  const now = Date.now();
  if (recordStartedAt === 0) recordStartedAt = now;

  // Structured-clone the message so later mutation of live store objects
  // cannot rewrite frames that were already captured. Anything unclonable is
  // not something the main thread could have received either, so dropping it
  // keeps the fixture honest.
  let snapshot: any;
  try {
    snapshot = typeof structuredClone === 'function' ? structuredClone(msg) : JSON.parse(JSON.stringify(msg));
  } catch {
    return;
  }

  frames.push({ atMs: now - recordStartedAt, msg: snapshot });
};

export const takePerfFixture = (): PerfFixture => ({
  version: 1,
  recordedAtMs: recordStartedAt,
  durationMs: frames.length ? frames[frames.length - 1].atMs : 0,
  frames,
});

export const stopReplay = (): void => {
  if (replayTimer) {
    clearTimeout(replayTimer);
    replayTimer = null;
  }
};

/**
 * Emit fixture frames at a FIXED cadence, ignoring their captured timestamps.
 *
 * Replaying the original inter-frame gaps would faithfully reproduce the
 * venue's burstiness — and burstiness is exactly what made runs incomparable.
 * A metronome trades realism for the ability to detect a 2% change.
 */
export const startReplay = (post: (msg: any) => void): void => {
  if (mode !== 'replay' || !replayFixture?.frames?.length) return;
  stopReplay();

  const tick = () => {
    if (mode !== 'replay' || !replayFixture) return;

    const frame = replayFixture.frames[replayCursor];
    if (frame) {
      try {
        post(frame.msg);
      } catch {
        // A dead port ends the run; nothing to recover.
      }
    }

    replayCursor += 1;
    if (replayCursor >= replayFixture.frames.length) {
      if (!replayLoop) return;
      replayCursor = 0;
    }

    replayTimer = setTimeout(tick, replayIntervalMs);
  };

  replayTimer = setTimeout(tick, replayIntervalMs);
};
