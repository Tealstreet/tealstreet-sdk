import { readFileSync } from 'fs';
import { join } from 'path';
import { describe, expect, it } from 'vitest';

// Reads source rather than importing index.ts, which parses argv on import.
const read = (relativePath: string) => readFileSync(join(__dirname, relativePath), 'utf8');

const topLevelCommandNames = (): string[] => {
  const index = read('index.ts');
  const direct = [...index.matchAll(/program\s*\.command\('([a-z-]+)/g)].map((m) => m[1]);
  const trading = [...index.matchAll(/\{ name: '([a-z-]+)', desc:/g)].map((m) => m[1]);
  const registered = [...read('commands/cross-trade.ts').matchAll(/\.command\('([a-z-]+)'\)/g)].map((m) => m[1]);
  return [...new Set([...direct, ...trading, ...registered.slice(0, 1)])];
};

const documentedSubcommandsTable = (): string => {
  const doc = read('../../documentation/cli/reference/commands.md');
  const start = doc.indexOf('### Subcommands');
  return doc.slice(start, doc.indexOf('\n## ', start));
};

describe('top-level CLI commands', () => {
  it('finds the commands it is meant to check', () => {
    expect(topLevelCommandNames()).toEqual(expect.arrayContaining(['login', 'account', 'buy', 'cross-trade']));
  });

  it('are all listed in the docs subcommand table', () => {
    const table = documentedSubcommandsTable();
    const missing = topLevelCommandNames().filter((name) => !table.includes(`\`${name}`));
    expect(missing).toEqual([]);
  });
});
