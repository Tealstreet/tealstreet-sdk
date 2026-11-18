import { afterEach, expect, it, vi } from 'vitest';

import { HyperliquidStartupFacts } from './hyperliquidStartupFacts';

afterEach(() => vi.useRealTimers());

it('counts waiting, prepared and in-flight reads without retaining request data', async () => {
  vi.useFakeTimers();
  const facts = new HyperliquidStartupFacts();
  const queued = facts.begin('direct', 20_000);
  let done!: () => void;
  const waiting = queued.wait(
    () =>
      new Promise<void>((resolve) => {
        done = resolve;
      }),
  );
  await vi.advanceTimersByTimeAsync(1500);
  expect(facts.tags()).toMatchObject({
    startup_info_route: 'direct',
    startup_info_read_timeout_ms: '20000',
    startup_info_budget_wait_ms: '1500',
    startup_info_budget_max_wait_ms: '1500',
    startup_info_budget_waiters: '1',
    startup_info_undispatched: '1',
    startup_info_in_flight: '0',
  });
  done();
  await waiting;
  expect(facts.tags().startup_info_budget_waiters).toBe('0');
  expect(facts.tags().startup_info_undispatched).toBe('1');
  queued.sent();
  expect(facts.tags()).toMatchObject({ startup_info_undispatched: '0', startup_info_in_flight: '1' });
  queued.response(429);
  queued.close();
  queued.close();
  expect(facts.tags()).toMatchObject({
    startup_info_http_429_count: '1',
    startup_info_in_flight: '0',
    startup_info_budget_wait_ms: '1500',
  });
});

it('closes cancelled waits and reports only closed route names and bounded counters', async () => {
  vi.useFakeTimers();
  const facts = new HyperliquidStartupFacts();
  const read = facts.begin('direct', 20_000);
  const controller = new AbortController();
  const waiting = read.wait(async () => {
    controller.signal.throwIfAborted();
  });
  await waiting;
  read.close();
  facts.begin('how', 20_000).close();
  expect(facts.tags()).toMatchObject({
    startup_info_route: 'mixed',
    startup_info_budget_waiters: '0',
    startup_info_undispatched: '0',
  });
  expect(
    Object.entries(facts.tags()).every(([key, value]) => key === 'startup_info_route' || /^\d+$/.test(value)),
  ).toBe(true);
});
