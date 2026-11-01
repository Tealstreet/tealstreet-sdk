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
    vi.mocked(axios.get).mockResolvedValue({ data: { time: new Date().toISOString(), unixMs: Date.now() } });

    await tealstreetTimeSource();

    expect(axios.get).toHaveBeenCalledWith('https://time.tealstreet.io/v1/time', { timeout: 5000 });
  });

  // The native app has no usable same-origin, so it names the edge route outright.
  it('uses a configured full endpoint URL verbatim', async () => {
    process.env.NEXT_PUBLIC_TIME_API_ORIGIN = 'https://preview.tealstreet.io/api/time/';
    vi.mocked(axios.get).mockResolvedValue({ data: { time: new Date().toISOString(), unixMs: Date.now() } });

    await tealstreetTimeSource();

    expect(axios.get).toHaveBeenCalledWith('https://preview.tealstreet.io/api/time', { timeout: 5000 });
  });

  it('reads unixMs, falling back to the ISO time', async () => {
    vi.mocked(axios.get).mockResolvedValue({ data: { unixMs: 1_000_000 } });
    expect(await tealstreetTimeSource()).toBeGreaterThanOrEqual(1_000_000);

    vi.mocked(axios.get).mockResolvedValue({ data: { time: '2026-01-01T00:00:00Z' } });
    expect(await tealstreetTimeSource()).toBeGreaterThanOrEqual(Date.parse('2026-01-01T00:00:00Z'));
  });

  it('returns undefined rather than throwing when the endpoint fails', async () => {
    vi.mocked(axios.get).mockRejectedValue(new Error('offline'));
    expect(await tealstreetTimeSource()).toBeUndefined();

    vi.mocked(axios.get).mockResolvedValue({ data: { time: 'not a date' } });
    expect(await tealstreetTimeSource()).toBeUndefined();
  });

  it('keeps the shortest of three round trips, so one slow response cannot skew the clock', async () => {
    vi.useFakeTimers();
    const truth = Date.parse('2026-11-01T00:00:00Z');
    vi.setSystemTime(truth);
    // Slow: the server stamps 2 s after the request leaves, then replies at once.
    // Fast: 50 ms each way. Both servers agree with the local clock.
    const slow = () => {
      vi.advanceTimersByTime(2000);
      return { data: { unixMs: Date.now() } };
    };
    const fast = () => {
      vi.advanceTimersByTime(50);
      const unixMs = Date.now();
      vi.advanceTimersByTime(50);
      return { data: { unixMs } };
    };
    vi.mocked(axios.get)
      .mockImplementationOnce(async () => slow())
      .mockImplementationOnce(async () => fast())
      .mockImplementationOnce(async () => slow());

    const serverNow = await tealstreetTimeSource();

    expect(axios.get).toHaveBeenCalledTimes(3);
    expect(Math.abs((serverNow as number) - Date.now())).toBeLessThanOrEqual(1);
    vi.useRealTimers();
  });

  it('uses the samples that answered when others fail', async () => {
    vi.mocked(axios.get)
      .mockRejectedValueOnce(new Error('timeout'))
      .mockResolvedValueOnce({ data: { unixMs: Date.now() } })
      .mockRejectedValueOnce(new Error('timeout'));
    expect(await tealstreetTimeSource()).toBeDefined();
  });
});
