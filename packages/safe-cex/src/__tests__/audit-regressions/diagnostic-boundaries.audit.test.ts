import { readdirSync, readFileSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';

import ts from 'typescript';
import { expect, it } from 'vitest';

it('keeps production console emissions behind the diagnostic boundary', () => {
  const root = resolve(process.cwd(), 'src');
  const raw: string[] = [];
  const walk = (directory: string) => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) {
        if (entry.name !== '__tests__' && entry.name !== 'node_modules') walk(path);
        continue;
      }
      if (!path.endsWith('.ts') || /\.(?:test|integration|network)\.ts$/.test(path)) continue;
      if (relative(root, path) === 'utils/diagnosticRedaction.ts') continue;
      const source = ts.createSourceFile(path, readFileSync(path, 'utf8'), ts.ScriptTarget.Latest, true);
      const visit = (node: ts.Node) => {
        if (ts.isPropertyAccessExpression(node)) {
          const receiver = node.expression;
          if (
            (ts.isIdentifier(receiver) && receiver.text === 'console') ||
            (ts.isPropertyAccessExpression(receiver) && receiver.getText(source) === 'globalThis.console')
          )
            raw.push(`${relative(root, path)}:${source.getLineAndCharacterOfPosition(node.getStart(source)).line + 1}`);
        }
        ts.forEachChild(node, visit);
      };
      visit(source);
    }
  };
  walk(root);
  expect(raw).toEqual([]);
});
