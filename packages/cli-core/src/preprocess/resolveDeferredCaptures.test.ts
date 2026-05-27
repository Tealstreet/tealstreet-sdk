import { describe, it, expect } from 'vitest';
import { resolveDeferredCaptures } from './substitute';

function store(vars: Record<string, string>) {
  return (name: string) => vars[name];
}

describe('resolveDeferredCaptures', () => {
  it('substitutes a populated capture var', () => {
    const { command, emptyCaptures } = resolveDeferredCaptures('cancel @o1', ['o1'], store({ o1: 'abc123,def456' }));
    expect(command).toBe('cancel abc123,def456');
    expect(emptyCaptures).toEqual([]);
  });

  // TSBUG-103: an empty capture must be reported so the runner can fail
  // closed — NOT collapse `cancel @o1` into a bare `cancel` (cancel-all).
  it('reports an empty capture var instead of silently dropping the token', () => {
    const { command, emptyCaptures } = resolveDeferredCaptures('cancel @o1', ['o1'], store({ o1: '' }));
    expect(command).toBe('cancel '); // token still removed...
    expect(emptyCaptures).toEqual(['o1']); // ...but the emptiness is surfaced
  });

  it('reports a missing capture var as empty', () => {
    const { emptyCaptures } = resolveDeferredCaptures('cancel @o1', ['o1'], store({}));
    expect(emptyCaptures).toEqual(['o1']);
  });

  it('collects multiple empty captures', () => {
    const { emptyCaptures } = resolveDeferredCaptures('cancel @o1,@o2', ['o1', 'o2'], store({ o1: '', o2: '' }));
    expect(emptyCaptures).toEqual(['o1', 'o2']);
  });

  it('only flags the empty one in a mixed chain', () => {
    const { command, emptyCaptures } = resolveDeferredCaptures(
      'cancel @o1 @o2',
      ['o1', 'o2'],
      store({ o1: 'abc123', o2: '' }),
    );
    expect(command).toBe('cancel abc123 ');
    expect(emptyCaptures).toEqual(['o2']);
  });

  it('respects the @o1 / @o10 boundary', () => {
    const { command } = resolveDeferredCaptures('cancel @o1 @o10', ['o1', 'o10'], store({ o1: 'A', o10: 'B' }));
    expect(command).toBe('cancel A B');
  });
});
