import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

export function sharedMarketDataSource(filename: string): string {
  const root = resolve(__dirname, '../../../..');
  const source = readFileSync(resolve(root, filename), 'utf8');
  const patch = readFileSync(resolve(root, 'sdk-rs/safe-cex-types/tests/market-data-shared.patch'), 'utf8');
  const section = patch.split(`diff --git a/${filename} b/${filename}\n`)[1].split('diff --git ')[0];
  const original = source.split('\n');
  const lines = section.split('\n');
  const output: string[] = [];
  let cursor = 0;
  for (let i = 0; i < lines.length; i++) {
    const hunk = /^@@ -(\d+)(?:,\d+)? \+\d+(?:,\d+)? @@/.exec(lines[i]);
    if (!hunk) continue;
    const start = Number(hunk[1]) - 1;
    output.push(...original.slice(cursor, start));
    cursor = start;
    while (++i < lines.length && !lines[i].startsWith('@@')) {
      const line = lines[i];
      if (!line) {
        if (original[cursor] === '') {
          output.push('');
          cursor++;
        }
        continue;
      }
      if (line.startsWith(' ') || line.startsWith('-')) {
        if (original[cursor++] !== line.slice(1))
          throw new Error(`Shared market-data patch context drift: ${filename}`);
      }
      if (line.startsWith(' ') || line.startsWith('+')) output.push(line.slice(1));
    }
    i--;
  }
  output.push(...original.slice(cursor));
  return output.join('\n');
}
