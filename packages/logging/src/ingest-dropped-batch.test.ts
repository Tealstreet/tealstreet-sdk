// @vitest-environment node
import { getDefaultStore } from 'jotai';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { logAtom } from './logAtom';
import { LogSeverity } from './LogSeverity';

vi.mock('@tealstreet/utils/src/compression', () => ({ compress: async (body: string) => body }));

let utils: typeof import('./utils');
let stop: () => void;
const fetchMock = vi.fn();

beforeEach(async () => {
  vi.useFakeTimers();
  vi.stubEnv('NODE_ENV', 'production');
  vi.stubEnv('NEXT_PUBLIC_LOG_INGEST_ORIGIN', '');
  vi.stubGlobal('fetch', fetchMock);
  vi.spyOn(console, 'error').mockImplementation(() => {});
  fetchMock.mockReset().mockResolvedValue(new Response(null, { status: 502 }));
  utils = await import('./utils');
  utils.resetLoggingStateForTests();
  getDefaultStore().set(logAtom, []);
  stop = utils.startLoggingToGrafana('synthetic-owner');
});
afterEach(() => {
  stop();
  vi.clearAllTimers();
  vi.useRealTimers();
  vi.unstubAllEnvs();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

it('records retry exhaustion in the existing local log and the next successful metrics batch without uploading a recursive notice', async () => {
  const canary = 'apiKey=synthetic-secret-not-for-the-notice';
  utils.logInfo(canary);
  utils.dumpLogQueue();
  await vi.advanceTimersByTimeAsync(31_000);
  expect(fetchMock).toHaveBeenCalledTimes(6);
  const notices = getDefaultStore().get(logAtom);
  expect(notices).toHaveLength(1);
  expect(notices[0]).toMatchObject({ severity: LogSeverity.Warning });
  expect(notices[0].message).toContain('1 log entries');
  expect(notices[0].message).toContain('HTTP 502');
  expect(JSON.stringify(notices)).not.toContain(canary);
  fetchMock.mockResolvedValue(new Response(null, { status: 204 }));
  await vi.advanceTimersByTimeAsync(600_000);
  utils.logInfo('healthy follow-up');
  utils.dumpLogQueue();
  await vi.advanceTimersByTimeAsync(0);
  expect(fetchMock).toHaveBeenCalledTimes(7);
  const values = JSON.parse(fetchMock.mock.calls[6][1].body).streams[0].values;
  const metrics = values
    .map((row: [string, string]) => row[1])
    .find((line: string) => line.includes('logging_metrics'));
  expect(JSON.parse(metrics)).toMatchObject({ droppedBatches: 1 });
  expect(values.some((row: [string, string]) => row[1].includes(notices[0].message))).toBe(false);
});

it('keeps a transient 502 with a successful retry out of the dropped-log notice', async () => {
  fetchMock
    .mockResolvedValueOnce(new Response(null, { status: 502 }))
    .mockResolvedValue(new Response(null, { status: 204 }));
  utils.logInfo('recoverable');
  utils.dumpLogQueue();
  await vi.advanceTimersByTimeAsync(1_000);
  expect(fetchMock).toHaveBeenCalledTimes(2);
  expect(getDefaultStore().get(logAtom)).toEqual([]);
});

it('does not surface an old owner failure after the login owner changes', async () => {
  let finish: (response: Response) => void = () => {};
  fetchMock.mockImplementationOnce(
    () =>
      new Promise<Response>((resolve) => {
        finish = resolve;
      }),
  );
  utils.logInfo('old owner');
  utils.dumpLogQueue();
  await vi.advanceTimersByTimeAsync(0);
  stop();
  stop = utils.startLoggingToGrafana('synthetic-successor');
  finish(new Response(null, { status: 502 }));
  await vi.advanceTimersByTimeAsync(31_000);
  expect(fetchMock).toHaveBeenCalledTimes(1);
  expect(getDefaultStore().get(logAtom)).toEqual([]);
});

it('bounds local drop notices and never copies a raw network exception into them', async () => {
  const canary = 'synthetic-private-exception';
  fetchMock.mockRejectedValue(new Error(canary));
  getDefaultStore().set(
    logAtom,
    Array.from({ length: 200 }, () => ({ timestamp: 0, message: 'existing', severity: LogSeverity.Info })),
  );
  utils.logInfo('lost');
  utils.dumpLogQueue();
  await vi.advanceTimersByTimeAsync(31_000);
  const entries = getDefaultStore().get(logAtom);
  expect(entries).toHaveLength(200);
  expect(entries[199].severity).toBe(LogSeverity.Warning);
  expect(entries[199].message).toContain('1 log entries');
  expect(entries[199].message).not.toContain(canary);
  expect(entries[199].message).not.toContain('HTTP');
  expect(fetchMock).toHaveBeenCalledTimes(6);
});

it('shows only a closed numeric upstream status from the ingest header', async () => {
  fetchMock.mockResolvedValue(new Response(null, { status: 502, headers: { 'X-Log-Ingest-Upstream-Status': '429' } }));
  utils.logInfo('lost');
  utils.dumpLogQueue();
  await vi.advanceTimersByTimeAsync(31_000);
  expect(getDefaultStore().get(logAtom)[0].message).toContain('upstream HTTP 429');
});

it('ignores malformed upstream status text instead of copying it into the local log', async () => {
  const canary = 'synthetic-secret-status';
  fetchMock.mockResolvedValue(new Response(null, { status: 502, headers: { 'X-Log-Ingest-Upstream-Status': canary } }));
  utils.logInfo('lost');
  utils.dumpLogQueue();
  await vi.advanceTimersByTimeAsync(31_000);
  const notice = getDefaultStore().get(logAtom)[0].message;
  expect(notice).toContain('HTTP 502');
  expect(notice).not.toContain('upstream');
  expect(notice).not.toContain(canary);
});

// AGT-67: resending a refusal only multiplied the load on a sink that would refuse it again.
it.each([
  [400, undefined],
  [403, undefined],
  [413, undefined],
  [429, undefined],
  [502, '400'],
  [502, '429'],
])('drops a refused batch (HTTP %s, upstream %s) without retrying', async (status, upstream) => {
  fetchMock.mockResolvedValue(
    new Response(null, { status, headers: upstream ? { 'X-Log-Ingest-Upstream-Status': upstream } : {} }),
  );
  utils.logInfo('refused');
  utils.dumpLogQueue();
  await vi.advanceTimersByTimeAsync(31_000);
  expect(fetchMock).toHaveBeenCalledTimes(1);
  const notices = getDefaultStore().get(logAtom);
  expect(notices).toHaveLength(1);
  expect(notices[0].message).toContain('refused');
  expect(notices[0].message).toContain('without retrying');
  expect(notices[0].message).toContain(`HTTP ${status}`);
});

it.each([
  [502, undefined],
  [502, '503'],
  [503, undefined],
  [408, undefined],
])('keeps retrying a failure that may recover (HTTP %s, upstream %s)', async (status, upstream) => {
  fetchMock.mockResolvedValue(
    new Response(null, { status, headers: upstream ? { 'X-Log-Ingest-Upstream-Status': upstream } : {} }),
  );
  utils.logInfo('transient');
  utils.dumpLogQueue();
  await vi.advanceTimersByTimeAsync(31_000);
  expect(fetchMock).toHaveBeenCalledTimes(6);
  expect(getDefaultStore().get(logAtom)[0].message).toContain('after 5 retries');
});

it('retries a 401 with a freshly requested ingest token', async () => {
  vi.stubEnv('NEXT_PUBLIC_LOG_INGEST_ORIGIN', 'https://log-ingest.example.test');
  stop();
  utils.resetLoggingStateForTests();
  stop = utils.startLoggingToGrafana('synthetic-owner');
  let tokens = 0;
  fetchMock.mockReset().mockImplementation(async (url: string) => {
    if (String(url).endsWith('/api/logs/token')) {
      tokens++;
      return Response.json({ token: `token-${tokens}`, expiresAt: Date.now() + 15 * 60_000 });
    }
    return new Response(null, { status: tokens === 1 ? 401 : 204 });
  });
  utils.logInfo('expired token');
  utils.dumpLogQueue();
  await vi.advanceTimersByTimeAsync(2_000);
  const uploads = fetchMock.mock.calls.filter(([url]) => String(url).endsWith('/v1/logs'));
  expect(uploads).toHaveLength(2);
  expect(uploads[1][1].headers.Authorization).toBe('Bearer token-2');
  expect(getDefaultStore().get(logAtom)).toEqual([]);
});
