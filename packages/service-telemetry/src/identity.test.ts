import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@sentry/bun', () => ({ init: vi.fn() }));
const Sentry = await import('@sentry/bun');

import { initServiceSentry, resetServiceSentryForTests } from './init';

const initOptions = () => vi.mocked(Sentry.init).mock.calls[0][0] as Record<string, any>;

beforeEach(() => {
  vi.clearAllMocks();
  resetServiceSentryForTests();
  vi.stubEnv('TEALSTREET_BOX', '');
  vi.stubEnv('TEALSTREET_RELEASE', '');
});
afterEach(() => vi.unstubAllEnvs());

const enable = (service: string, release?: string) =>
  initServiceSentry({
    dsn: 'https://public@o1.ingest.us.sentry.io/2',
    service,
    release,
    tags: { region: 'eu', box: 'stale-box' },
  });

for (const service of ['exchange-server', 'exchange-gateway']) {
  describe(service, () => {
    it('attributes both Sentry fields to the configured box and the image release', () => {
      vi.stubEnv('TEALSTREET_BOX', ' exchange-02 ');
      vi.stubEnv('TEALSTREET_RELEASE', ' fb652156d52 ');
      enable(service);
      expect(initOptions().serverName).toBe('exchange-02');
      expect(initOptions().release).toBe('fb652156d52');
      expect(initOptions().initialScope.tags).toEqual({ service, box: 'exchange-02' });
      const event = initOptions().beforeSend({
        server_name: 'ephemeral-container',
        release: 'undefined',
        tags: { region: 'eu', box: 'old', venue: 'bybit' },
      });
      expect(event.server_name).toBe('exchange-02');
      expect(event.release).toBe('fb652156d52');
      expect(event.tags).toEqual({ box: 'exchange-02', venue: 'bybit' });
    });

    it.each([undefined, '', ' ', 'undefined', 'null', 'unknown'])(
      'omits missing or placeholder identity %s',
      (value) => {
        vi.stubEnv('TEALSTREET_BOX', value);
        vi.stubEnv('TEALSTREET_RELEASE', value);
        enable(service, value);
        expect(initOptions().serverName).toBeUndefined();
        expect(initOptions().release).toBeUndefined();
        expect(initOptions().initialScope.tags).toEqual({ service });
        const event = initOptions().beforeSend({
          server_name: 'container',
          release: 'undefined',
          tags: { region: 'eu', box: 'stale' },
        });
        expect(event).not.toHaveProperty('server_name');
        expect(event).not.toHaveProperty('release');
        expect(event.tags).toEqual({});
      },
    );

    it('uses the runtime image revision when a wrapper supplied a placeholder', () => {
      vi.stubEnv('TEALSTREET_RELEASE', 'a1b2c3d4');
      enable(service, 'undefined');
      expect(initOptions().release).toBe('a1b2c3d4');
      expect(initOptions().beforeSend({}).release).toBe('a1b2c3d4');
    });

    it('keeps an explicit image release and refuses a malformed box label', () => {
      vi.stubEnv('TEALSTREET_BOX', 'not a box\nlabel');
      vi.stubEnv('TEALSTREET_RELEASE', 'env-revision');
      enable(service, 'image-revision');
      expect(initOptions().release).toBe('image-revision');
      expect(initOptions().serverName).toBeUndefined();
    });
  });
}

it('preserves other services and never leaks the machine hostname', () => {
  vi.stubEnv('TEALSTREET_BOX', 'exchange-01');
  vi.stubEnv('TEALSTREET_RELEASE', 'exchange-revision');
  enable('candle-server', 'candle-revision');
  expect(initOptions().release).toBe('candle-revision');
  expect(initOptions().initialScope.tags).toEqual({ service: 'candle-server', region: 'eu', box: 'stale-box' });
  expect(initOptions().beforeSend({ server_name: 'container' })).not.toHaveProperty('server_name');
});
