import axios from 'axios';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { getTealstreetTimeEndpoint, tealstreetTimeSource } from './tealstreetTimeSource';

vi.mock('axios', () => ({ default: { get: vi.fn() } }));

describe('tealstreetTimeSource', () => {
  afterEach(() => {
    vi.clearAllMocks();
    delete process.env.NEXT_PUBLIC_TIME_API_ORIGIN;
  });

  it('defaults to the same-origin edge route', () => {
    expect(getTealstreetTimeEndpoint()).toMatch(/\/api\/time$/);
  });

  it('uses the configured public endpoint instead of the Vercel route', async () => {
    process.env.NEXT_PUBLIC_TIME_API_ORIGIN = 'https://time.tealstreet.io///';
    vi.mocked(axios.get).mockResolvedValueOnce({ data: { time: new Date().toISOString(), unixMs: Date.now() } });

    await tealstreetTimeSource();

    expect(axios.get).toHaveBeenCalledWith('https://time.tealstreet.io/v1/time', { timeout: 5000 });
  });

  // The native app has no usable same-origin, so it names the edge route outright.
  it('uses a configured full endpoint URL verbatim', async () => {
    process.env.NEXT_PUBLIC_TIME_API_ORIGIN = 'https://preview.tealstreet.io/api/time/';
    vi.mocked(axios.get).mockResolvedValueOnce({ data: { time: new Date().toISOString(), unixMs: Date.now() } });

    await tealstreetTimeSource();

    expect(axios.get).toHaveBeenCalledWith('https://preview.tealstreet.io/api/time', { timeout: 5000 });
  });

  it('reads unixMs, falling back to the ISO time', async () => {
    vi.mocked(axios.get).mockResolvedValueOnce({ data: { unixMs: 1_000_000 } });
    expect(await tealstreetTimeSource()).toBeGreaterThanOrEqual(1_000_000);

    vi.mocked(axios.get).mockResolvedValueOnce({ data: { time: '2026-01-01T00:00:00Z' } });
    expect(await tealstreetTimeSource()).toBeGreaterThanOrEqual(Date.parse('2026-01-01T00:00:00Z'));
  });

  it('returns undefined rather than throwing when the endpoint fails', async () => {
    vi.mocked(axios.get).mockRejectedValueOnce(new Error('offline'));
    expect(await tealstreetTimeSource()).toBeUndefined();

    vi.mocked(axios.get).mockResolvedValueOnce({ data: { time: 'not a date' } });
    expect(await tealstreetTimeSource()).toBeUndefined();
  });
});
