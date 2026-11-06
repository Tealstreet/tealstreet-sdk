import { beforeEach, afterEach, expect, it, vi } from 'vitest';

const probe = vi.hoisted(() => ({ envelopes: [] as any[] }));
vi.mock('@sentry/bun', async (original) => {
  const actual = await original<typeof import('@sentry/bun')>();
  return {
    ...actual,
    init: (options: any) =>
      actual.init({
        ...options,
        transport: () => ({
          send: async (envelope: unknown) => {
            probe.envelopes.push(envelope);
            return { statusCode: 200 };
          },
          flush: async () => true,
        }),
      }),
  };
});
import * as Sentry from '@sentry/bun';
import { initServiceSentry, resetServiceSentryForTests } from './init';
import { recordServiceMetrics } from './capture';

beforeEach(() => {
  resetServiceSentryForTests();
  for (const scope of [Sentry.getCurrentScope(), Sentry.getIsolationScope()]) {
    scope.setAttributes({
      'server.address': undefined,
      'sentry.release': undefined,
      region: undefined,
      box: undefined,
    });
    scope.setSession(undefined);
  }
  probe.envelopes = [];
  vi.stubEnv('TEALSTREET_RELEASE', '');
  vi.stubEnv('SENTRY_RELEASE', '');
  vi.stubEnv('SENTRY_NAME', 'review-inferred-host');
});
afterEach(async () => {
  await Sentry.close(1000);
  vi.unstubAllEnvs();
});

for (const service of ['exchange-server', 'exchange-gateway']) {
  for (const box of ['exchange-01', '']) {
    it(`review real SDK ${service} box=${box || 'unset'} excludes inferred host`, async () => {
      vi.stubEnv('TEALSTREET_BOX', box);
      initServiceSentry({ dsn: 'https://public@o1.ingest.us.sentry.io/2', service });
      Sentry.captureEvent({ message: 'review' });
      recordServiceMetrics({ prefix: 'review', counts: { seen: 1 }, gauges: {}, attributes: {} });
      await Sentry.flush(1000);
      const items = probe.envelopes.flatMap((envelope) => envelope[1]);
      const event = items.find(([header]: any) => header.type === 'event')?.[1];
      expect(event).toBeDefined();
      expect(event.server_name).toBe(box || undefined);
      const metricItem = items.find(([header]: any) => header.type === 'trace_metric')?.[1];
      expect(metricItem, JSON.stringify(items.map(([h]: any) => h))).toBeDefined();
      const metrics = typeof metricItem === 'string' ? JSON.parse(metricItem) : metricItem;
      const metric = metrics.items?.[0] ?? metrics[0] ?? metrics;
      expect(metric.attributes['server.address']?.value ?? metric.attributes['server.address']).toBe(box || undefined);
    });
  }
}

for (const service of ['exchange-server', 'exchange-gateway']) {
  it(`review real SDK ${service} excludes placeholder release in metrics`, async () => {
    vi.stubEnv('TEALSTREET_BOX', 'exchange-01');
    vi.stubEnv('SENTRY_RELEASE', 'undefined');
    initServiceSentry({ dsn: 'https://public@o1.ingest.us.sentry.io/2', service });
    Sentry.captureEvent({ message: 'release review' });
    recordServiceMetrics({ prefix: 'review', counts: { seen: 1 }, gauges: {}, attributes: {} });
    await Sentry.flush(1000);
    const items = probe.envelopes.flatMap((envelope) => envelope[1]);
    const event = items.find(([header]: any) => header.type === 'event')?.[1];
    expect(event).toBeDefined();
    expect(event.release).toBeUndefined();
    const metrics = items.find(([header]: any) => header.type === 'trace_metric')?.[1];
    expect(metrics?.items?.[0]).toBeDefined();
    const metric = metrics.items[0];
    expect(metric.attributes['sentry.release']?.value ?? metric.attributes['sentry.release']).toBeUndefined();
  });
}

for (const service of ['exchange-server', 'exchange-gateway']) {
  for (const box of ['exchange-01', '']) {
    for (const revision of ['image-revision-123', '']) {
      it(`real SDK ${service} box=${box || 'unset'} release=${revision || 'unset'} normalizes events, metrics and sessions`, async () => {
        vi.stubEnv('TEALSTREET_BOX', box);
        vi.stubEnv('TEALSTREET_RELEASE', revision);
        vi.stubEnv('SENTRY_RELEASE', 'undefined');
        initServiceSentry({
          dsn: 'https://public@o1.ingest.us.sentry.io/2',
          service,
          release: 'undefined',
          tags: { region: 'eu' },
        });
        expect((Sentry.getClient()?.getOptions() as { serverName?: string }).serverName).toBe(box || '');
        expect(Sentry.getClient()?.getOptions().release).toBe(revision || undefined);
        Sentry.captureEvent({ message: 'identity matrix', server_name: 'inferred-host', release: 'undefined' });
        recordServiceMetrics({
          prefix: 'matrix',
          counts: { seen: 1 },
          gauges: {},
          attributes: {
            'server.address': 'caller-host',
            'sentry.release': 'undefined',
            region: 'eu',
            box: 'stale',
          },
        });
        Sentry.startSession({ release: 'undefined' });
        Sentry.captureSession();
        Sentry.getClient()?.sendSession({
          attrs: { release: 'undefined' },
          aggregates: [{ started: new Date().toISOString(), exited: 1 }],
        });
        await Sentry.flush(1000);
        const items = probe.envelopes.flatMap((envelope) => envelope[1]);
        const event = items.find(([header]: any) => header.type === 'event')?.[1];
        expect(event.server_name).toBe(box || undefined);
        expect(event.release).toBe(revision || undefined);
        expect(event.tags.region).toBeUndefined();
        const metric = items.find(([header]: any) => header.type === 'trace_metric')?.[1].items[0];
        expect(metric.attributes['server.address']?.value).toBe(box || undefined);
        expect(metric.attributes['sentry.release']?.value).toBe(revision || undefined);
        expect(metric.attributes.region).toBeUndefined();
        expect(metric.attributes.box?.value).toBe(box || undefined);
        const sessions = items.filter(([header]: any) => ['session', 'sessions'].includes(header.type));
        if (revision) {
          expect(sessions.length).toBeGreaterThanOrEqual(2);
          for (const [, session] of sessions) expect(session.attrs.release).toBe(revision);
        } else expect(sessions).toHaveLength(0);
      });
    }
  }
  it(`real SDK ${service} removes current metric scope identity`, async () => {
    vi.stubEnv('TEALSTREET_BOX', '');
    vi.stubEnv('SENTRY_RELEASE', 'undefined');
    initServiceSentry({ dsn: 'https://public@o1.ingest.us.sentry.io/2', service });
    Sentry.getCurrentScope().setAttributes({
      'server.address': 'scope-host',
      'sentry.release': 'undefined',
      region: 'eu',
      box: 'scope-box',
    });
    recordServiceMetrics({ prefix: 'scope', counts: { seen: 1 }, gauges: {}, attributes: {} });
    await Sentry.flush(1000);
    const items = probe.envelopes.flatMap((envelope) => envelope[1]);
    const metric = items.find(([header]: any) => header.type === 'trace_metric')?.[1].items[0];
    for (const key of ['server.address', 'sentry.release', 'region', 'box'])
      expect(metric.attributes[key]).toBeUndefined();
  });
}

for (const service of ['exchange-server', 'exchange-gateway']) {
  it(`review working real SDK ${service} configured identities override current scope`, async () => {
    vi.stubEnv('TEALSTREET_BOX', 'exchange-01');
    vi.stubEnv('TEALSTREET_RELEASE', 'image-revision-123');
    initServiceSentry({ dsn: 'https://public@o1.ingest.us.sentry.io/2', service });
    Sentry.getCurrentScope().setAttributes({
      'server.address': 'current-scope-host',
      'sentry.release': 'undefined',
      box: 'scope-box',
      region: 'eu',
      venue: 'bitget',
    });
    Object.assign(Sentry.getClient()!.getOptions(), { enableLogs: true });
    Sentry.logger.info('configured identity log');
    recordServiceMetrics({ prefix: 'working', counts: { seen: 1 }, gauges: {}, attributes: {} });
    await Sentry.flush(1000);
    const metric = probe.envelopes
      .flatMap((envelope) => envelope[1])
      .find(([header]: any) => header.type === 'trace_metric')?.[1].items[0];
    const log = probe.envelopes.flatMap((envelope) => envelope[1]).find(([header]: any) => header.type === 'log')?.[1]
      .items[0];
    expect(log).toBeDefined();
    for (const item of [metric, log]) {
      expect(item.attributes['server.address']?.value).toBe('exchange-01');
      expect(item.attributes['sentry.release']?.value).toBe('image-revision-123');
      expect(item.attributes.box?.value).toBe('exchange-01');
      expect(item.attributes.region).toBeUndefined();
      expect(item.attributes.venue?.value).toBe('bitget');
    }
  });
  it(`review real SDK ${service} final log scope identity cannot bypass normalization`, async () => {
    vi.stubEnv('TEALSTREET_BOX', '');
    vi.stubEnv('TEALSTREET_RELEASE', '');
    initServiceSentry({ dsn: 'https://public@o1.ingest.us.sentry.io/2', service });
    Object.assign(Sentry.getClient()!.getOptions(), { enableLogs: true });
    Sentry.getCurrentScope().setAttributes({
      'server.address': 'current-scope-host',
      'sentry.release': 'undefined',
      region: 'eu',
      box: 'scope-box',
    });
    Sentry.logger.info('review scope log');
    await Sentry.flush(1000);
    const log = probe.envelopes.flatMap((envelope) => envelope[1]).find(([header]: any) => header.type === 'log')?.[1]
      .items[0];
    expect(log).toBeDefined();
    for (const key of ['server.address', 'sentry.release', 'region', 'box'])
      expect.soft(log.attributes[key]).toBeUndefined();
  });
}

for (const service of ['exchange-server', 'exchange-gateway']) {
  for (const configured of [false, true]) {
    it(`review real SDK ${service} drops transaction envelope configured=${configured}`, async () => {
      vi.stubEnv('TEALSTREET_BOX', configured ? 'exchange-01' : '');
      vi.stubEnv('TEALSTREET_RELEASE', configured ? 'image-revision-123' : '');
      initServiceSentry({ dsn: 'https://public@o1.ingest.us.sentry.io/2', service });
      const event = {
        type: 'transaction' as const,
        transaction: 'identity-review',
        start_timestamp: 1,
        timestamp: 2,
        server_name: 'transaction-host',
        release: 'undefined',
        tags: { box: 'scope-box', region: 'eu', venue: 'bitget' },
      };
      Sentry.captureEvent(event);
      await Sentry.flush(1000);
      const tx = probe.envelopes.flatMap((e) => e[1]).find(([h]: any) => h.type === 'transaction')?.[1];
      expect(tx).toBeUndefined();
    });
    it(`review real SDK ${service} drops streamed span envelope configured=${configured}`, async () => {
      vi.stubEnv('TEALSTREET_BOX', configured ? 'exchange-01' : '');
      vi.stubEnv('TEALSTREET_RELEASE', configured ? 'image-revision-123' : '');
      initServiceSentry({ dsn: 'https://public@o1.ingest.us.sentry.io/2', service });
      Object.assign(Sentry.getClient()!.getOptions(), { tracesSampleRate: 1 });
      Sentry.getCurrentScope().setAttributes({
        'server.address': 'span-host',
        'sentry.release': 'undefined',
        box: 'scope-box',
        region: 'eu',
        venue: 'bitget',
      });
      Sentry.startSpan({ name: 'identity-span-review', forceTransaction: true }, () => {});
      await Sentry.flush(1000);
      const spans = probe.envelopes
        .flatMap((e) => e[1])
        .filter(([h]: any) => h.type === 'span')
        .flatMap(([, p]: any) => p.items);
      expect(spans).toHaveLength(0);
    });
  }
}

for (const service of ['exchange-server', 'exchange-gateway']) {
  for (const configured of [false, true]) {
    it(`review real SDK ${service} mixed adjacent trace items configured=${configured}`, async () => {
      vi.stubEnv('TEALSTREET_BOX', configured ? 'exchange-01' : '');
      vi.stubEnv('TEALSTREET_RELEASE', configured ? 'image-revision-123' : '');
      initServiceSentry({ dsn: 'https://public@o1.ingest.us.sentry.io/2', service });
      const attrs = () => ({
        'server.address': { type: 'string', value: 'old-host' },
        'sentry.release': { type: 'string', value: 'undefined' },
        box: { type: 'string', value: 'old-box' },
        region: { type: 'string', value: 'eu' },
        venue: { type: 'string', value: 'bitget' },
      });
      const metric = { items: [{ attributes: attrs() }, { attributes: attrs() }] };
      const log = { items: [{ attributes: attrs() }, { attributes: attrs() }] };
      const report = { discarded_events: [{ reason: 'sample_rate', category: 'error', quantity: 1 }] };
      const attachment = new Uint8Array([1, 2, 3]);
      const env: any = [
        {},
        [
          [{ type: 'transaction' }, { server_name: 'old-host' }],
          [{ type: 'span' }, { items: [{ attributes: attrs() }] }],
          [{ type: 'span' }, { items: [{ attributes: attrs() }] }],
          [{ type: 'trace_metric' }, metric],
          [{ type: 'transaction' }, { server_name: 'old-host' }],
          [{ type: 'log' }, log],
          [{ type: 'client_report' }, report],
          [{ type: 'attachment', filename: 'fixture.bin' }, attachment],
          [{ type: 'session' }, { attrs: { release: configured ? 'image-revision-123' : undefined } }],
          [{ type: 'sessions' }, { attrs: { release: configured ? 'image-revision-123' : undefined }, aggregates: [] }],
          [{ type: 'span' }, { items: [] }],
        ],
      ];
      await Sentry.getClient()!.sendEnvelope(env);
      expect(probe.envelopes).toHaveLength(1);
      const out = probe.envelopes[0][1];
      expect(out.map(([h]: any) => h.type)).toEqual(
        configured ? ['trace_metric', 'log', 'session', 'sessions'] : ['trace_metric', 'log'],
      );
      for (const container of [metric, log])
        for (const item of container.items) {
          expect(item.attributes.venue).toEqual({ type: 'string', value: 'bitget' });
          expect((item.attributes as any)['server.address']?.value).toBe(configured ? 'exchange-01' : undefined);
          expect((item.attributes as any)['sentry.release']?.value).toBe(configured ? 'image-revision-123' : undefined);
          expect((item.attributes as any).box?.value).toBe(configured ? 'exchange-01' : undefined);
          expect((item.attributes as any).region).toBeUndefined();
        }
      expect(out.find(([h]: any) => h.type === 'client_report')).toBeUndefined();
      expect(out.find(([h]: any) => h.type === 'attachment')).toBeUndefined();
    });
    it(`review real SDK ${service} drops feedback configured=${configured}`, async () => {
      vi.stubEnv('TEALSTREET_BOX', configured ? 'exchange-01' : '');
      vi.stubEnv('TEALSTREET_RELEASE', configured ? 'image-revision-123' : '');
      initServiceSentry({ dsn: 'https://public@o1.ingest.us.sentry.io/2', service });
      Sentry.getCurrentScope().addEventProcessor((event) => ({
        ...event,
        server_name: 'feedback-host',
        release: 'undefined',
      }));
      Sentry.captureEvent({
        message: 'ordinary error control',
        tags: { box: 'scope-box', region: 'eu', venue: 'bitget' },
      });
      Sentry.captureFeedback({ message: 'review feedback', tags: { box: 'scope-box', region: 'eu', venue: 'bitget' } });
      await Sentry.flush(1000);
      const normal = probe.envelopes.flatMap((e) => e[1]).find(([h]: any) => h.type === 'event')?.[1];
      expect(normal).toBeDefined();
      expect(normal.server_name).toBe(configured ? 'exchange-01' : undefined);
      expect(normal.release).toBe(configured ? 'image-revision-123' : undefined);
      expect(normal.tags.box).toBe(configured ? 'exchange-01' : undefined);
      expect(normal.tags.region).toBeUndefined();
      expect(normal.tags.venue).toBe('bitget');
      const item = probe.envelopes.flatMap((e) => e[1]).find(([h]: any) => h.type === 'feedback')?.[1];
      expect(item).toBeUndefined();
    });
  }
}

for (const service of ['exchange-server', 'exchange-gateway'])
  for (const configured of [false, true]) {
    it(`review ${service} drops unsupported check-in configured=${configured}`, async () => {
      vi.stubEnv('TEALSTREET_BOX', configured ? 'exchange-01' : '');
      vi.stubEnv('TEALSTREET_RELEASE', configured ? 'image-revision-123' : '');
      initServiceSentry({ dsn: 'https://public@o1.ingest.us.sentry.io/2', service });
      Sentry.getCurrentScope().setAttributes({
        'server.address': 'scope-host',
        'sentry.release': 'undefined',
        box: 'scope-box',
        region: 'eu',
      });
      Sentry.captureCheckIn({ monitorSlug: 'identity-review', status: 'ok' });
      await Sentry.flush(1000);
      const checkIn = probe.envelopes.flatMap((e) => e[1]).find(([h]: any) => h.type === 'check_in')?.[1];
      expect(checkIn).toBeUndefined();
    });
  }

for (const service of ['exchange-server', 'exchange-gateway']) {
  it(`real SDK ${service} drops future unknown envelope types without dropping metrics`, async () => {
    vi.stubEnv('TEALSTREET_BOX', '');
    initServiceSentry({ dsn: 'https://public@o1.ingest.us.sentry.io/2', service });
    const metric = { items: [{ attributes: { venue: { type: 'string', value: 'bitget' } } }] };
    await Sentry.getClient()!.sendEnvelope([
      {},
      [
        [{ type: 'future_identity_item' }, { server_name: 'old-host', release: 'undefined' }],
        [{ type: 'trace_metric' }, metric],
      ],
    ] as any);
    expect(probe.envelopes).toHaveLength(1);
    expect(probe.envelopes[0][1]).toEqual([[{ type: 'trace_metric' }, metric]]);
  });
}
