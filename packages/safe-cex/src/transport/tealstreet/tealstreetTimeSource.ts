import axios from 'axios';
import dayjs from 'dayjs';
import utc from 'dayjs/plugin/utc';

dayjs.extend(utc);

const getBaseUrl = (): string => (typeof self !== 'undefined' && self.location?.origin ? self.location.origin : '');

/**
 * Accepts either a bare origin for the dedicated time service, which serves
 * `/v1/time`, or a full endpoint URL. The full-URL form exists for the native
 * app, whose `self.location.origin` is the Metro dev server: it names the edge
 * `/api/time` (~70ms) rather than the CloudFront-fronted service (~750ms).
 */
export const getTealstreetTimeEndpoint = (): string => {
  const configured = process.env.NEXT_PUBLIC_TIME_API_ORIGIN?.trim().replace(/\/+$/, '');
  if (!configured) return `${getBaseUrl()}/api/time`;
  return /\/time$/.test(configured) ? configured : `${configured}/v1/time`;
};

export const tealstreetTimeSource = async (): Promise<number | undefined> => {
  try {
    const requestedAt = Date.now();
    const {
      data: { time, unixMs },
    } = await axios.get(getTealstreetTimeEndpoint(), { timeout: 5000 });
    const receivedAt = Date.now();
    const serverMs = typeof unixMs === 'number' ? unixMs : dayjs.utc(time).valueOf();
    if (!Number.isFinite(serverMs)) return undefined;
    // The endpoint stamps immediately before responding, so half the round
    // trip removes the systematic network lag without overshooting.
    return serverMs + Math.floor((receivedAt - requestedAt) / 2);
  } catch {
    return undefined;
  }
};
