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
  const metrics = values.map((row: [string, string]) => row[1]).find((line: string) => line.includes('logging_metrics'));
  expect(JSON.parse(metrics)).toMatchObject({ droppedBatches: 1 });
  expect(values.some((row: [string, string]) => row[1].includes(notices[0].message))).toBe(false);
});

it('keeps a transient 502 with a successful retry out of the dropped-log notice', async () => {
  fetchMock.mockResolvedValueOnce(new Response(null, { status: 502 })).mockResolvedValue(new Response(null, { status: 204 }));
  utils.logInfo('recoverable');
  utils.dumpLogQueue();
  await vi.advanceTimersByTimeAsync(1_000);
  expect(fetchMock).toHaveBeenCalledTimes(2);
  expect(getDefaultStore().get(logAtom)).toEqual([]);
});

it('does not surface an old owner failure after the login owner changes', async () => {
  let finish: (response: Response) => void = () => {};
  fetchMock.mockImplementationOnce(() => new Promise<Response>(resolve => { finish = resolve; }));
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
