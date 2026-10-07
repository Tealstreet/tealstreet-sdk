import { describe, expect, it } from 'vitest';

import { buildCommandCatalog } from './catalog';

describe('buildCommandCatalog', () => {
  const catalog = buildCommandCatalog();

  it('describes the trading commands an agent needs', () => {
    const names = catalog.map((entry) => entry.name);
    expect(names).toEqual(expect.arrayContaining(['buy', 'sell', 'close', 'cancel', 'status']));
  });

  it('gives every command a description and a unique name', () => {
    expect(catalog.filter((entry) => !entry.description.trim())).toEqual([]);
    expect(new Set(catalog.map((entry) => entry.name)).size).toBe(catalog.length);
  });
});
