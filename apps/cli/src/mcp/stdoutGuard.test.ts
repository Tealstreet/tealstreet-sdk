import { describe, expect, it, vi } from 'vitest';

import { isMcpInvocation } from './stdoutGuard';

describe('isMcpInvocation', () => {
  it('detects the mcp subcommand', () => {
    expect(isMcpInvocation(['bun', 'index.ts', 'mcp'])).toBe(true);
  });

  it('is false for other invocations', () => {
    expect(isMcpInvocation(['bun', 'index.ts'])).toBe(false);
    expect(isMcpInvocation(['bun', 'index.ts', 'account', 'list'])).toBe(false);
  });

  // Several commands take `-a, --account <name>`. A loose includes('mcp')
  // would silently redirect stdout to stderr for a user whose account is
  // named `mcp`, breaking `>` redirection and `| jq` on a normal command.
  it('does not fire on an account or symbol that happens to be named mcp', () => {
    expect(isMcpInvocation(['bun', 'index.ts', 'buy', 'BTC', '1', '-a', 'mcp'])).toBe(false);
    expect(isMcpInvocation(['bun', 'index.ts', 'account', 'default', 'mcp'])).toBe(false);
  });
});

describe('installStdoutGuard', () => {
  /**
   * Patching console.log alone is not enough: every script tool calls
   * loadScripts(), which dynamic-imports scripts already on disk that
   * checkTopLevelEffects never saw. Those can reach stdout via console.table,
   * console.dir, or process.stdout.write directly. The guard therefore has to
   * hold at the stream level.
   */
  it('redirects every stdout writer to stderr, including console.table and raw writes', async () => {
    vi.resetModules();
    const { installStdoutGuard } = await import('./stdoutGuard');

    const realStdoutWrite = process.stdout.write;
    const realStderrWrite = process.stderr.write;
    const stderrChunks: string[] = [];
    let leakedToStdout = 0;

    // Capture at the ORIGINAL functions, before the guard swaps them.
    process.stdout.write = ((chunk: unknown) => {
      leakedToStdout += 1;
      void chunk;
      return true;
    }) as typeof process.stdout.write;
    process.stderr.write = ((chunk: unknown) => {
      stderrChunks.push(String(chunk));
      return true;
    }) as typeof process.stderr.write;

    try {
      const { protocolStdout } = installStdoutGuard();

      // NB: vitest intercepts `console.*` before it reaches
      // process.stdout.write, so console output cannot be asserted here. The
      // contract that matters is the stream-level one below — in production
      // every console writer funnels through process.stdout.write, which is
      // exactly what this guard replaces.
      console.log('a log line');
      console.table?.([{ a: 1 }]);
      process.stdout.write('a raw write\n');

      expect(leakedToStdout).toBe(0);
      expect(stderrChunks.join('')).toContain('a raw write');

      // The transport still needs a path to the REAL stdout, or JSON-RPC
      // would land on stderr and the client would see nothing at all.
      protocolStdout.write('protocol frame\n');
      expect(leakedToStdout).toBe(1);
    } finally {
      process.stdout.write = realStdoutWrite;
      process.stderr.write = realStderrWrite;
    }
  });

  it('leaves console.error on stderr', async () => {
    vi.resetModules();
    const { installStdoutGuard } = await import('./stdoutGuard');
    const before = console.error;
    installStdoutGuard();
    expect(console.error).toBe(before);
  });
});
