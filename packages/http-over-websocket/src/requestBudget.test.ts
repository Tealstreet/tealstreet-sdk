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
  it('accepts closed engine labels for posted reads and refuses arbitrary labels', () => {
    expect(requestClass('POST', 'https://venue.test/private', 'history')).toBe('history');
    expect(requestClass('GET', 'https://venue.test/read', 'constructor')).toBe('read');
    expect(requestClass('GET', 'https://venue.test/read', 'credential-sentinel')).toBe('read');
    expect(budgetDiagnostic('history', 30000, 30012.8)).toBe('class=history budget=30000ms elapsed=30012ms');
  });
});
