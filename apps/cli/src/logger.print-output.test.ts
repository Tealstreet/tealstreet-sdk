import { afterEach, describe, expect, it, vi } from 'vitest';

vi.mock('./storage-context', () => ({
  getStorageDir: () => '/tmp/cli-logger-test',
  onStorageOwnerChange: () => () => {},
}));
vi.mock('fs', async (importOriginal) => {
  const actual = await importOriginal<typeof import('fs')>();
  return {
    ...actual,
    existsSync: () => true,
    mkdirSync: () => undefined,
    writeFileSync: () => undefined,
    appendFileSync: () => undefined,
  };
});
vi.mock('./status-line', () => ({ setStatus: () => undefined }));

// `tasks list` printed the same Type:/Status: lines and blank spacers for each
// task; the 5/s limit and 10 s dedup dropped the second task entirely.
describe('subcommand output through printOutput', () => {
  const restore: Array<() => void> = [];
  afterEach(() => restore.splice(0).forEach((undo) => undo()));

  async function intercepted() {
    const terminal: unknown[][] = [];
    const original = console.log;
    console.log = (...args: unknown[]) => void terminal.push(args);
    vi.resetModules();
    const logger = await import('./logger');
    logger.interceptConsole();
    restore.push(() => {
      logger.restoreConsole();
      console.log = original;
    });
    return { terminal, logger };
  }

  const lines = Array.from({ length: 30 }, (_, i) => (i % 3 === 2 ? '' : `    Type: ${i % 2 ? 'chase' : 'twap'}`));

  it('prints every line, repeats and blanks included', async () => {
    const { terminal, logger } = await intercepted();
    lines.forEach((line) => logger.printOutput(line));
    expect(terminal.map((args) => args[0])).toEqual(lines);
  });

  it('still limits plain console.log, which carries exchange noise', async () => {
    const { terminal } = await intercepted();
    lines.forEach((line) => console.log(line));
    expect(terminal.length).toBeLessThan(lines.length);
  });
});
