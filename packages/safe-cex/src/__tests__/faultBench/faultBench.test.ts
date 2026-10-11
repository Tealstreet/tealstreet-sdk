import axios, { AxiosError } from 'axios';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { createFakeVenue } from './fakeVenue';
import { deferred } from './deferred';
import { createFaultAdapter } from './faultAdapter';

const setup = () => {
  const book: unknown[] = [];
  const venue = createFakeVenue({
    'post /order': ({ body }) => {
      book.push(body);
      return { status: 200, data: { accepted: body } };
    },
  });
  const bench = createFaultAdapter(venue);
  const xhr = axios.create();
  bench.install(xhr);
  return { venue, bench, xhr, book };
};

const failure = (promise: Promise<unknown>) =>
  promise.then(
    () => undefined,
    (error: AxiosError) => error,
  );

describe('fault bench', () => {
  afterEach(() => vi.useRealTimers());

  it('delivers by default and records what the venue received', async () => {
    const { venue, xhr } = setup();
    const response = await xhr.post('/order?x=1', { qty: 1 });

    expect(response.data).toEqual({ accepted: { qty: 1 } });
    expect(venue.received).toMatchObject([{ method: 'post', path: '/order', body: { qty: 1 } }]);
  });

  it('drops a request before the venue sees it', async () => {
    const { venue, bench, xhr } = setup();
    bench.next('dropBeforeSend');

    const error = await failure(xhr.post('/order', { qty: 1 }));
    expect(error?.code).toBe('ECONNRESET');
    expect(venue.received).toHaveLength(0);
  });

  it('loses the response of a request the venue acted on', async () => {
    const { venue, bench, xhr } = setup();
    bench.next('loseResponse');

    const error = await failure(xhr.post('/order', { qty: 1 }));
    expect(error?.code).toBe('ECONNRESET');
    expect(error?.response).toBeUndefined();
    expect(venue.received).toHaveLength(1);
  });

  it('times out a request the venue acted on', async () => {
    const { venue, bench, xhr } = setup();
    bench.next('timeout');

    expect((await failure(xhr.post('/order', {})))?.code).toBe('ECONNABORTED');
    expect(venue.received).toHaveLength(1);
  });

  it('answers a rejection, and an unknown route, as HTTP errors', async () => {
    const { bench, xhr } = setup();
    bench.next({ reject: 429, data: { msg: 'slow down' } });

    const rejected = await failure(xhr.post('/order', {}));
    expect(rejected?.response?.status).toBe(429);
    expect(rejected?.response?.data).toEqual({ msg: 'slow down' });
    expect((await failure(xhr.get('/missing')))?.response?.status).toBe(404);
  });

  it('records a rejected request without acting on it', async () => {
    const { venue, bench, xhr, book } = setup();
    bench.next({ reject: 400 });

    await failure(xhr.post('/order', { qty: 1 }));
    expect(venue.received).toHaveLength(1);
    expect(book).toHaveLength(0);
  });

  it('acts on a held request at once and replies when released', async () => {
    const { bench, xhr, book } = setup();
    const release = deferred<void>();
    bench.next({ hold: release.promise });

    let replied = false;
    const held = xhr.post('/order', { qty: 1 }).then(() => (replied = true));
    await vi.waitFor(() => expect(book).toHaveLength(1));
    expect(replied).toBe(false);
    release.resolve();
    await held;
    expect(replied).toBe(true);
  });

  it('serves keyed faults to their route before the unkeyed script', async () => {
    const { bench, xhr } = setup();
    bench.next('dropBeforeSend');
    bench.on('get /missing', { reject: 503 });

    expect((await failure(xhr.get('/missing')))?.response?.status).toBe(503);
    expect((await failure(xhr.post('/order', {})))?.code).toBe('ECONNRESET');
  });

  it('holds a request for its latency, and consumes faults in order', async () => {
    vi.useFakeTimers();
    const { bench, xhr } = setup();
    bench.next({ latencyMs: 5_000 }, 'dropBeforeSend');

    let settled = false;
    const slow = xhr.post('/order', {}).then(() => (settled = true));
    await vi.advanceTimersByTimeAsync(4_999);
    expect(settled).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    await slow;
    expect(settled).toBe(true);

    expect((await failure(xhr.post('/order', {})))?.code).toBe('ECONNRESET');
    expect(bench.pending()).toBe(0);
  });
});
