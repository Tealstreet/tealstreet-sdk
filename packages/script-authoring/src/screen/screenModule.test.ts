import type { OpenAiJsonClient } from '../openai';

import { describe, expect, it, vi } from 'vitest';

import { AiProviderError } from '../openai';
import { screenModule } from './screenModule';

const fakeClient = (response: unknown) =>
  ({ complete: vi.fn(async () => response) }) as unknown as OpenAiJsonClient & { complete: ReturnType<typeof vi.fn> };

const run = (code: string, response: unknown, extra: Partial<{ name: string; description: string }> = {}) => {
  const client = fakeClient(response);
  return {
    client,
    result: screenModule(
      { code, name: extra.name ?? 'Panel', description: extra.description ?? 'Shows things', capabilities: '' },
      { model: 'm', client, nonce: 'fixed-nonce' },
    ),
  };
};

const ALLOW = { verdict: 'allow', reason: '', concerns: [] };

describe('the ordering invariant — static findings are final', () => {
  /**
   * The single most important behaviour in this file. If the model could clear
   * a static finding, a comment in the source saying "this is approved" would
   * be a working bypass — prompt injection into our own reviewer.
   */
  it('does not let the model clear a static block', async () => {
    const { result } = run(`const c = document.cookie;`, ALLOW);
    expect((await result).allowed).toBe(false);
  });

  it('does not even call the provider when a static rule blocks', async () => {
    const { client, result } = run(`const c = document.cookie;`, ALLOW);
    await result;
    expect(client.complete).not.toHaveBeenCalled();
  });

  it('names the offending line to the author', async () => {
    const { result } = run(`const a = 1;\nconst c = document.cookie;`, ALLOW);
    const { reason } = await result;
    expect(reason).toMatch(/line 2/);
    expect(reason).toMatch(/cookie/i);
  });

  it('lets the model ADD a block on code that passes the static scan', async () => {
    const { result } = run(`const Component = () => { api.orders.closeAllPositions(); return null; };`, {
      verdict: 'block',
      reason: 'Closes every position on mount, which a viewer should not do.',
      concerns: [{ severity: 'block', detail: 'Trades on mount.' }],
    });
    const screened = await result;
    expect(screened.allowed).toBe(false);
    expect(screened.reason).toMatch(/Closes every position/);
  });

  /**
   * A model that says "allow" while raising a block-severity concern is
   * contradicting itself. Disagreement is not a reason to trust the softer half.
   */
  it('blocks when the verdict and the concerns disagree', async () => {
    const { result } = run(`const Component = () => null;`, {
      verdict: 'allow',
      reason: '',
      concerns: [{ severity: 'block', detail: 'Silently sells the position.' }],
    });
    expect((await result).allowed).toBe(false);
  });
});

describe('fail closed on anything unexpected', () => {
  it('throws rather than allowing when the verdict is unreadable', async () => {
    await expect(run(`const Component = () => null;`, { verdict: 'maybe' }).result).rejects.toBeInstanceOf(
      AiProviderError,
    );
  });

  it('throws rather than allowing on a null response', async () => {
    await expect(run(`const Component = () => null;`, null).result).rejects.toBeInstanceOf(AiProviderError);
  });

  it('always gives a refused module some reason, even when the model gave none', async () => {
    const { result } = run(`const Component = () => null;`, { verdict: 'block', reason: '   ', concerns: [] });
    const screened = await result;
    expect(screened.allowed).toBe(false);
    expect(screened.reason).toBeTruthy();
  });

  it('ignores malformed entries in the concerns array rather than throwing', async () => {
    const { result } = run(`const Component = () => null;`, {
      verdict: 'allow',
      reason: '',
      concerns: [null, 'nope', { detail: 42 }, { severity: 'flag', detail: 'Reads balances.' }],
    });
    const screened = await result;
    expect(screened.allowed).toBe(true);
    expect(screened.concerns).toEqual([{ severity: 'flag', detail: 'Reads balances.' }]);
  });

  // An unrecognised severity must not silently become the permissive one by
  // accident — but it must also not be treated as a block on a typo.
  it('normalizes an unknown severity to flag', async () => {
    const { result } = run(`const Component = () => null;`, {
      verdict: 'allow',
      reason: '',
      concerns: [{ severity: 'catastrophic', detail: 'Something.' }],
    });
    expect((await result).concerns[0].severity).toBe('flag');
  });
});

describe('the ordinary path', () => {
  it('allows a plain panel and reports no reason', async () => {
    const { result } = run(`const Component = () => <div>{api.data.usePositions().length}</div>;`, ALLOW);
    const screened = await result;
    expect(screened.allowed).toBe(true);
    expect(screened.reason).toBeNull();
    expect(screened.modelSkipped).toBe(false);
  });

  it('passes the static findings to the model as facts', async () => {
    const { client, result } = run(`fetch('/x');\nconst Component = () => null;`, ALLOW);
    await result;
    const sent = client.complete.mock.calls[0][0] as { input: string };
    expect(sent.input).toContain('network-fetch');
    expect(sent.input).toContain('STATIC ANALYSIS FINDINGS');
  });

  it('carries flagged findings through even when allowed', async () => {
    const { result } = run(`fetch('/x');\nconst Component = () => null;`, ALLOW);
    const screened = await result;
    expect(screened.allowed).toBe(true);
    expect(screened.findings.map((f) => f.rule)).toContain('network-fetch');
  });

  it('fences the source with the nonce so it cannot close its own delimiter', async () => {
    const { client, result } = run(`const Component = () => null;`, ALLOW);
    await result;
    const sent = client.complete.mock.calls[0][0] as { input: string };
    expect(sent.input).toContain('MODULE_SOURCE_fixed-nonce');
  });

  it('gives the model the declared purpose to compare against', async () => {
    const { client, result } = run(`const Component = () => null;`, ALLOW, {
      name: 'Portfolio Viewer',
      description: 'Read-only view of open positions',
    });
    await result;
    const sent = client.complete.mock.calls[0][0] as { input: string };
    expect(sent.input).toContain('Portfolio Viewer');
    expect(sent.input).toContain('Read-only view of open positions');
  });
});

describe('tiers', () => {
  it('puts an ordinary panel in the strict tier', async () => {
    const { result } = run(`const Component = () => <div>hi</div>;`, ALLOW);
    const screened = await result;
    expect(screened.tier).toBe('strict');
    expect(screened.allowed).toBe(true);
    expect(screened.reason).toBeNull();
  });

  // fetch earns the standard frame, which allows the network and still syncs.
  it('allows a module that fetches, in the standard tier', async () => {
    const { result } = run(`fetch('https://api.coinglass.com/x');\nconst Component = () => null;`, ALLOW);
    const screened = await result;
    expect(screened.allowed).toBe(true);
    expect(screened.tier).toBe('standard');
    expect(screened.reason).toBeNull();
  });

  /**
   * The ordering invariant, applied to the network rather than to blocks. A
   * static standard finding stands whatever the model says, so nothing written
   * in the source can argue a networked module into the strict frame.
   */
  it('does not let the model lower a static standard finding', async () => {
    const { result } = run(`fetch('/x');\nconst Component = () => null;`, {
      verdict: 'allow',
      reason: 'Totally ordinary panel, no network needed.',
      concerns: [],
    });
    expect((await result).tier).toBe('standard');
  });

  it('lets the model RAISE a clean module to standard', async () => {
    const { result } = run(`const Component = () => null;`, {
      verdict: 'allow',
      reason: '',
      concerns: [{ severity: 'standard', detail: 'Builds a request URL from user input.' }],
    });
    const screened = await result;
    expect(screened.allowed).toBe(true);
    expect(screened.tier).toBe('standard');
  });

  it('reports a blocked module as the blocked tier', async () => {
    const { result } = run(`const c = document.cookie;`, ALLOW);
    expect((await result).tier).toBe('blocked');
  });

  it('reports a model refusal as the blocked tier too', async () => {
    const { result } = run(`const Component = () => null;`, {
      verdict: 'block',
      reason: 'Sells everything on mount.',
      concerns: [],
    });
    const screened = await result;
    expect(screened.tier).toBe('blocked');
    expect(screened.allowed).toBe(false);
  });

  // An invented severity is a malformed answer; it must not become a refusal
  // or a downgrade. The static rules carry the hard decisions.
  it('normalizes an unknown concern severity down to flag, not up', async () => {
    const { result } = run(`const Component = () => null;`, {
      verdict: 'allow',
      reason: '',
      concerns: [{ severity: 'catastrophic', detail: 'Something.' }],
    });
    const screened = await result;
    expect(screened.concerns[0].severity).toBe('flag');
    expect(screened.tier).toBe('strict');
  });
});
