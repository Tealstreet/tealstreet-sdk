import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

/**
 * ASCII is a terminal rendering: under an ASCII terminal (TEALSTREET_ASCII=1 on
 * a TTY) machine JSON, redirected output and capture/listener payloads must
 * still carry the canonical Unicode text byte for byte.
 */
const terminal: string[] = [];
vi.mock('./logger', () => ({
  originalConsoleLog: (...args: unknown[]) => void terminal.push(args.join(' ')),
  originalConsoleError: (...args: unknown[]) => void terminal.push(args.join(' ')),
  originalConsoleWarn: (...args: unknown[]) => void terminal.push(args.join(' ')),
  writeAbovePrompt: (write: () => void) => write(),
  isStdoutReservedForJson: () => false,
  reserveStdoutForJson: () => undefined,
  logger: { info: () => undefined, warn: () => undefined, error: () => undefined, debug: () => undefined },
}));
vi.mock('./status-line', () => ({ setStatus: () => undefined }));

const CANONICAL = '✓ 東京 → Δ café ⏸️  Paused';

function asTerminal(isTTY: boolean) {
  const env = process.env.TEALSTREET_ASCII;
  const tty = Object.getOwnPropertyDescriptor(process.stdout, 'isTTY');
  process.env.TEALSTREET_ASCII = '1';
  Object.defineProperty(process.stdout, 'isTTY', { value: isTTY, configurable: true });
  return () => {
    if (env === undefined) delete process.env.TEALSTREET_ASCII;
    else process.env.TEALSTREET_ASCII = env;
    if (tty) Object.defineProperty(process.stdout, 'isTTY', tty);
    else delete (process.stdout as { isTTY?: boolean }).isTTY;
  };
}

describe('ASCII terminal sinks', () => {
  let restore: () => void = () => undefined;
  beforeEach(() => {
    terminal.length = 0;
    vi.resetModules();
  });
  afterEach(() => {
    restore();
    vi.restoreAllMocks();
  });

  it('a --json document round-trips non-ASCII text exactly', async () => {
    restore = asTerminal(true);
    const writes: unknown[] = [];
    vi.spyOn(process.stdout, 'write').mockImplementation(((chunk: unknown, done?: () => void) => {
      writes.push(chunk);
      done?.();
      return true;
    }) as typeof process.stdout.write);
    const { GLYPH_MODE } = await import('./console-glyphs');
    const { emitJson } = await import('./jsonMode');
    expect(GLYPH_MODE).toBe('ascii');
    const document = { account: '東京 → Δ', alias: 'café', marker: '✓', control: '\x1b[31mred\x1b[0m' };
    await emitJson(document);
    expect(writes).toEqual([`${JSON.stringify(document)}\n`]);
    expect(JSON.parse(String(writes[0]))).toEqual(document);
  });

  it('redirected output stays byte-exact', async () => {
    restore = asTerminal(false);
    const { GLYPH_MODE, G, U, terminalText } = await import('./console-glyphs');
    expect(GLYPH_MODE).toBe('unicode');
    expect(G).toBe(U);
    expect(terminalText(CANONICAL)).toBe(CANONICAL);
    const adapter = (await import('./platform-adapter')).createStandalonePlatformAdapter();
    adapter.print([CANONICAL]);
    expect(terminal).toEqual([CANONICAL]);
  });

  it('capture and listener sinks get canonical text; only the terminal branch is ASCII', async () => {
    restore = asTerminal(true);
    const { runWithAdapterContext } = await import('./listener/adapter-context');
    const adapter = (await import('./platform-adapter')).createStandalonePlatformAdapter();

    const stop = adapter.startCapture();
    adapter.printInfo(CANONICAL);
    expect(stop()).toContain(CANONICAL);

    const sink: string[] = [];
    await runWithAdapterContext(
      { printSink: (_kind: string, line: string) => void sink.push(line) } as never,
      async () => adapter.print([CANONICAL]),
    );
    expect(sink).toEqual([CANONICAL]);
    expect(terminal).toEqual([]);

    adapter.print([CANONICAL]);
    expect(terminal).toEqual(['+ 東京 > Δ café ||  Paused']);
  });

  it('record list hands the adapter canonical rows', async () => {
    restore = asTerminal(true);
    vi.doMock('./record/index', () => ({
      initDatabase: () => undefined,
      getActiveRecorders: () => [],
      getRecordings: () => [
        {
          active: true,
          dataType: 'orderbook',
          exchange: 'bybit',
          symbol: 'BTCUSDT',
          intervalMs: 1000,
          retentionDays: 7,
        },
      ],
    }));
    const { RecordCommand } = await import('./commands/record/RecordCommand');
    const output: string[] = [];
    await new RecordCommand().execute({
      args: { _: ['record', 'list'] },
      adapter: {
        printInfo: (text: string) => void output.push(text),
        printError: (text: string) => {
          throw new Error(text);
        },
      },
    } as never);
    expect(output.join('\n')).toContain('⏸️  Paused');
  });
});
