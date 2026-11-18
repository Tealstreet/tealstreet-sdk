import { describe, expect, it } from 'vitest';

import { redactDiagnosticData, redactDiagnosticDataWithOutcome } from '../diagnostic-redaction';

const omitted = '[unserializable diagnostic]';

describe('closed diagnostic omission outcomes', () => {
  it.each([
    ['parser_budget', () => Array.from({ length: 65 }, (_, i) => `[entry ${i}]`)],
    ['serialized_size', () => '[' + 'x'.repeat(65_536)],
    [
      'serialized_depth',
      () => {
        let text = '{}';
        for (let i = 0; i < 5; i++) text = JSON.stringify({ error: text });
        return text;
      },
    ],
  ] as const)('reports %s without exposing input', (reason, makeInput) => {
    const input = { context: 'public synthetic marker', detail: makeInput() };
    const result = redactDiagnosticDataWithOutcome(input);
    expect(result).toMatchObject({
      status: 'omitted',
      value: omitted,
      omission: { reason, stage: 'parser_inspection', byteCountCapped: false },
    });
    if (result.status !== 'omitted') throw new Error('Expected omission');
    expect(result.omission.inspectedStringBytes).toBeGreaterThan(0);
    expect(Object.keys(result.omission).sort()).toEqual(['byteCountCapped', 'inspectedStringBytes', 'reason', 'stage']);
    expect(JSON.stringify(result)).not.toContain('public synthetic marker');
    expect(redactDiagnosticData(input)).toBe(omitted);
  });

  it.each(['parser_inspection', 'copy'] as const)('classifies hostile reads at %s without re-reading', (stage) => {
    let reads = 0;
    const input = Object.defineProperty({}, 'detail', {
      enumerable: true,
      get() {
        reads++;
        if (stage === 'parser_inspection' || reads === 2) throw new Error('private synthetic marker');
        return 'ok';
      },
    });
    expect(redactDiagnosticDataWithOutcome(input)).toEqual({
      status: 'omitted',
      value: omitted,
      omission: {
        reason: 'inspection_failure',
        stage,
        inspectedStringBytes: stage === 'copy' ? 2 : 0,
        byteCountCapped: false,
      },
    });
    expect(reads).toBe(stage === 'copy' ? 2 : 1);
  });

  it('does not inspect or coerce a thrown value to classify it', () => {
    const thrown = new Proxy(
      {},
      {
        get() {
          throw new Error('must not inspect thrown value');
        },
      },
    );
    const input = Object.defineProperty({}, 'detail', {
      enumerable: true,
      get() {
        throw thrown;
      },
    });
    expect(redactDiagnosticDataWithOutcome(input)).toMatchObject({
      status: 'omitted',
      omission: { reason: 'inspection_failure', stage: 'parser_inspection' },
    });
  });

  it('counts UTF-8 string bytes once and bounds the metric without encoding the whole input', () => {
    const input = { same: 'é😀', again: 'é😀', broken: '[' };
    Object.defineProperty(input, 'late', {
      enumerable: true,
      get() {
        throw new Error('closed');
      },
    });
    // Object.values fails before yielding siblings: there are no inspected strings.
    expect(redactDiagnosticDataWithOutcome(input)).toMatchObject({ omission: { inspectedStringBytes: 0 } });
    const first = {
      detail: 'é😀',
      same: 'é😀',
      nested: Object.defineProperty({}, 'late', {
        enumerable: true,
        get() {
          throw 0;
        },
      }),
    };
    expect(redactDiagnosticDataWithOutcome(first)).toMatchObject({
      omission: { inspectedStringBytes: 6, byteCountCapped: false },
    });
    const huge = redactDiagnosticDataWithOutcome('[' + '😀'.repeat(600_000));
    expect(huge).toMatchObject({
      omission: { reason: 'serialized_size', inspectedStringBytes: 1_048_576, byteCountCapped: true },
    });
  });

  it('retains successful copy identity semantics, cycles, privacy and literal omission-shaped user text', () => {
    const input = { symbol: 'BTCUSDT', apiKey: 'synthetic-key', text: omitted, self: undefined as unknown };
    input.self = input;
    const result = redactDiagnosticDataWithOutcome(input);
    expect(result.status).toBe('redacted');
    expect(result.value).toMatchObject({ symbol: 'BTCUSDT', apiKey: '[REDACTED]', text: omitted });
    expect(result.value).not.toBe(input);
    if (result.status === 'redacted') expect(result.value.self).toBe(result.value);
    expect(input.apiKey).toBe('synthetic-key');
    expect(result).not.toHaveProperty('omission');
    expect(redactDiagnosticDataWithOutcome(omitted)).toEqual({ status: 'redacted', value: omitted });
  });

  it('does not leak an omission outcome into the next invocation', () => {
    expect(redactDiagnosticDataWithOutcome('['.repeat(65))).toHaveProperty('status', 'omitted');
    expect(redactDiagnosticDataWithOutcome({ count: 3 })).toEqual({ status: 'redacted', value: { count: 3 } });
  });
});
