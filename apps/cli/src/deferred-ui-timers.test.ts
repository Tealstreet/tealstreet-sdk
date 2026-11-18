import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { mkdtempSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { sessionFixture } from './deferred-test.fixture';
const resize = vi.hoisted((): Array<() => void> => []);
vi.mock('./scroll-region', () => ({
  addResizeListener: (fn: () => void) => {
    resize.push(fn);
    return () => {};
  },
  reserveTop() {},
  reserveBottom() {},
  resumeRegion() {},
  suspendRegion() {},
}));
const tty = Object.getOwnPropertyDescriptor(process.stdout, 'isTTY');
beforeEach(() => {
  vi.resetModules();
  vi.useFakeTimers();
  resize.length = 0;
  const root = mkdtempSync(join(tmpdir(), 'cli-owner-ui-'));
  vi.stubEnv('TEALSTREET_CONFIG_DIR', root);
  vi.stubEnv('TEALSTREET_CONFIG_FILE', join(root, 'config.json'));
  Object.defineProperty(process.stdout, 'isTTY', { configurable: true, value: true });
});
afterEach(async () => {
  (await import('./footer')).disableFooter();
  (await import('./status-line')).disableStatusLine();
  vi.restoreAllMocks();
  vi.useRealTimers();
  vi.unstubAllEnvs();
  if (tty) Object.defineProperty(process.stdout, 'isTTY', tty);
  else Reflect.deleteProperty(process.stdout, 'isTTY');
});
it.each(['same', 'B', 'away and back'])(
  'drops saved footer redraw and resize callbacks during login: %s',
  async (change) => {
    const { login } = await sessionFixture();
    const footer = await import('./footer');
    const interval = vi.spyOn(globalThis, 'setInterval');
    const write = vi.spyOn(process.stdout, 'write').mockReturnValue(true);
    let frame = 'A-start';
    footer.setFooterRenderer(() => ({ left: frame, right: '' }));
    footer.enableFooter();
    const tick = interval.mock.calls[0][0],
      oldResize = resize[0];
    if (change !== 'same') {
      login('B');
      if (change === 'away and back') login('A');
      footer.setFooterRenderer(() => ({ left: 'B-current', right: '' }));
      footer.enableFooter();
    }
    write.mockClear();
    frame = 'A-late';
    if (typeof tick === 'function') tick();
    oldResize();
    const text = write.mock.calls.map((call) => String(call[0])).join('');
    if (change === 'same') expect(text).toContain('A-late');
    else expect(text).toBe('');
  },
);
it.each(['same', 'B', 'away and back'])(
  'drops old status dim and resize callbacks without touching B status: %s',
  async (change) => {
    const { login } = await sessionFixture();
    const status = await import('./status-line');
    const timeout = vi.spyOn(globalThis, 'setTimeout');
    const write = vi.spyOn(process.stdout, 'write').mockReturnValue(true);
    status.enableStatusLine();
    status.setStatus('A-warning', 'warn');
    const dim = timeout.mock.calls.at(-1)?.[0],
      oldResize = resize[0];
    if (change !== 'same') {
      login('B');
      if (change === 'away and back') login('A');
      status.setStatus('B-warning', 'warn');
    }
    write.mockClear();
    if (typeof dim === 'function') dim();
    oldResize();
    const text = write.mock.calls.map((call) => String(call[0])).join('');
    if (change === 'same') expect(text).toContain('A-warning');
    else {
      expect(text).toBe('');
      resize.at(-1)?.();
      expect(write.mock.calls.map((call) => String(call[0])).join('')).toContain('B-warning');
    }
  },
);
