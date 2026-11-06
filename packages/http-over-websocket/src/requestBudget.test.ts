import { describe, expect, it } from 'vitest';
import fixtures from './requestBudget.fixtures.json';
import { budgetDiagnostic, requestBudgetMs, requestClass, REQUEST_BUDGET_MS, type RequestClass } from './requestBudget';

describe('bounded HTTP class budgets (Rust parity oracle)', () => {
  it.each(fixtures)('$method $url $timeoutMs', (row) => {
    const kind = requestClass(row.method, row.url);
    expect(kind).toBe(row.requestClass);
    expect(requestBudgetMs(kind, row.timeoutMs)).toBe(row.budgetMs);
  });
  it.each(Object.keys(REQUEST_BUDGET_MS) as RequestClass[])('%s cannot acquire an unbounded timer', (kind) => {
    for (const invalid of [undefined, 0, -1, NaN, Infinity, -Infinity]) {
      expect(requestBudgetMs(kind, invalid)).toBe(REQUEST_BUDGET_MS[kind]);
    }
    expect(requestBudgetMs(kind, Number.MAX_VALUE)).toBe(2_147_483_647);
    expect(requestBudgetMs(kind, 0.1)).toBe(1);
  });
  it.each(['POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'post'])('%s cannot get a longer default from a hint', method => {
    for (const hint of Object.keys(REQUEST_BUDGET_MS)) {
      const kind = requestClass(method, 'https://venue.test/markets', hint);
      expect(kind).toBe('orderWrite');
      expect(requestBudgetMs(kind)).toBe(20_000);
    }
    // Explicit caller deadlines remain authoritative; this only hardens hints.
    expect(requestBudgetMs(requestClass(method, 'https://venue.test/read', 'history'), 1234)).toBe(1234);
  });

  it('accepts closed GET labels and refuses arbitrary labels', () => {
    expect(requestClass('GET', 'https://venue.test/private', 'history')).toBe('history');
    expect(requestClass('GET', 'https://venue.test/read', 'constructor')).toBe('read');
    expect(requestClass('GET', 'https://venue.test/read', 'credential-sentinel')).toBe('read');
    expect(budgetDiagnostic('history', 30000, 30012.8)).toBe('class=history budget=30000ms elapsed=30012ms');
  });
});
