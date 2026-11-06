import axios from 'axios';
import dayjs from 'dayjs';
import utc from 'dayjs/plugin/utc';

dayjs.extend(utc);

const getBaseUrl = (): string => (typeof self !== 'undefined' && self.location?.origin ? self.location.origin : '');

/**
 * Same-origin `/api/time`, unless a full endpoint URL is configured: the native
 * app names one because its `self.location.origin` is the Metro dev server. A bare
 * origin, the old `time.tealstreet.io` form, is ignored so that host can retire.
 */
export const getTealstreetTimeEndpoint = (): string => {
  const configured = process.env.NEXT_PUBLIC_TIME_API_ORIGIN?.trim().replace(/\/+$/, '');
  return configured && /\/time$/.test(configured) ? configured : `${getBaseUrl()}/api/time`;
};

const SAMPLES = 3;

type TimeSample = { offsetMs: number; roundTripMs: number };

const sampleTime = async (endpoint: string): Promise<TimeSample | undefined> => {
  try {
    const requestedAt = Date.now();
    const {
      data: { time, unixMs },
    } = await axios.get(endpoint, { timeout: 5000 });
    const receivedAt = Date.now();
    const serverMs = typeof unixMs === 'number' ? unixMs : dayjs.utc(time).valueOf();
    if (!Number.isFinite(serverMs)) return undefined;
    // The endpoint stamps immediately before responding, so half the round
    // trip removes the systematic network lag without overshooting.
    const roundTripMs = receivedAt - requestedAt;
    return { offsetMs: serverMs + Math.floor(roundTripMs / 2) - receivedAt, roundTripMs };
  } catch {
    return undefined;
  }
};

/**
 * Keeps the shortest of three round trips: a slow response is delayed before the
 * server stamps it, so it reads late by about half the delay, and one 2 s outlier
 * would cross virtualClock's 1 s threshold and shift every signed timestamp.
 */
export const tealstreetTimeSource = async (): Promise<number | undefined> => {
  const endpoint = getTealstreetTimeEndpoint();
  let best: TimeSample | undefined;
  for (let i = 0; i < SAMPLES; i++) {
    const sample = await sampleTime(endpoint);
    if (sample && (!best || sample.roundTripMs < best.roundTripMs)) best = sample;
  }
  return best ? Date.now() + best.offsetMs : undefined;
};
