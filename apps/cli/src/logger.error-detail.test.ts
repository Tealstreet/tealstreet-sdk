import { afterEach, describe, expect, it, vi } from 'vitest';

// These tests isolate terminal/file formatting; profile routing has real-FS tests.
vi.mock('./storage-context', () => ({
  getStorageDir: () => '/tmp/cli-logger-test',
  onStorageOwnerChange: () => () => {},
}));

const fileLines: string[] = [];
vi.mock('fs', async (importOriginal) => {
  const actual = await importOriginal<typeof import('fs')>();
  return {
    ...actual,
    existsSync: () => true,
    mkdirSync: () => undefined,
    // The logger truncates ~/.tealstreet/cli.log at import; a fresh CI runner has no such dir.
    writeFileSync: () => undefined,
    appendFileSync: (_path: unknown, line: string) => void fileLines.push(line),
  };
});

const statuses: string[] = [];
vi.mock('./status-line', () => ({ setStatus: (message: string) => void statuses.push(message) }));

/**
 * The live-user regression: 0.10.17 printed "✗ bybitdemo error" twice (the line
 * above the prompt and the red status bar) because the failed envelope's cause
 * sat in fields the terminal extractor did not read.
 */
describe('console.error through the CLI logger', () => {
  const restore: Array<() => void> = [];
  afterEach(() => {
    restore.splice(0).forEach((undo) => undo());
    fileLines.length = 0;
    statuses.length = 0;
  });

  async function intercepted() {
    const terminal: unknown[][] = [];
    const original = console.error;
    console.error = (...args: unknown[]) => void terminal.push(args);
    vi.resetModules();
    const logger = await import('./logger');
    logger.interceptConsole();
    restore.push(() => {
      logger.restoreConsole();
      console.error = original;
    });
    return terminal;
  }

  it('prints the cause on both surfaces, never the bare prefix', async () => {
    const terminal = await intercepted();
    console.error('bybitdemo error', { raw_response: '{"retCode":10006,"retMsg":"Too many visits!"}' });
    expect(terminal.map((args) => args[1])).toEqual(['bybitdemo error Too many visits!']);
    expect(statuses).toEqual(['bybitdemo error Too many visits!']);
    // The file log keeps the full body.
    expect(fileLines.join('\n')).toContain('raw_response');
  });

  it('serializes a body with no message or code', async () => {
    const terminal = await intercepted();
    console.error('bybitdemo error', { result: null, time: 1 });
    expect(terminal.map((args) => args[1])).toEqual(['bybitdemo error {"result":null,"time":1}']);
  });
});
