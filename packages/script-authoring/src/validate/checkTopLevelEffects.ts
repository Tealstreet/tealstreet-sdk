import ts from 'typescript';

import type { ValidationIssue } from './validateScript';

/**
 * Flag code that EXECUTES at module top level.
 *
 * SPEC §5.1 documents the hazard: `scripts-store` loads scripts with
 * `import()`, so anything outside the default-exported function runs at load
 * time — at REPL startup and on every `script reload` — before any capability
 * gate applies. That was an accepted risk when the author was a human typing
 * into their own filesystem. It is a different proposition when an agent
 * writes the file, which is why this exists.
 *
 * ## Scope: calls and awaits, NOT declarations
 *
 * `checkRuntimeImports` already rejects every non-type import, so the classic
 * example from the SPEC — `import { exec } from 'child_process'` — is already
 * a hard error and needs nothing here. What remains reachable at top level
 * without any import: `Bun.spawn(...)`, `fetch(...)`, `globalThis.process.*`,
 * and top-level `await`.
 *
 * So this deliberately allows declarations. Helper `function`s, `const` config
 * objects, type aliases, enums and `export const meta` are all legitimate at
 * top level and rejecting them would break real scripts for no security gain.
 * Only *evaluation* is flagged.
 */
export const checkTopLevelEffects = (source: string): ValidationIssue[] => {
  const file = ts.createSourceFile('/script.ts', source, ts.ScriptTarget.ESNext, true);
  const issues: ValidationIssue[] = [];

  const positionOf = (node: ts.Node): Pick<ValidationIssue, 'line' | 'column'> => {
    const { line, character } = file.getLineAndCharacterOfPosition(node.getStart(file));
    return { line: line + 1, column: character + 1 };
  };

  /** Walk an initializer/expression WITHOUT descending into function bodies —
   *  a call inside a helper function only runs when that helper is called. */
  const scanExpression = (node: ts.Node, describe: (n: ts.Node) => void): void => {
    if (ts.isFunctionLike(node)) return;
    if (ts.isCallExpression(node) || ts.isNewExpression(node) || ts.isAwaitExpression(node)) {
      describe(node);
      return;
    }
    ts.forEachChild(node, (child) => scanExpression(child, describe));
  };

  const report = (node: ts.Node, what: string): void => {
    issues.push({
      message: `${what} runs at module load time, before the script is invoked and before any capability gate applies. Move it inside the default-exported function.`,
      ...positionOf(node),
    });
  };

  for (const statement of file.statements) {
    // Declarations are fine — only evaluation is a hazard.
    if (
      ts.isImportDeclaration(statement) ||
      ts.isExportDeclaration(statement) ||
      ts.isFunctionDeclaration(statement) ||
      ts.isClassDeclaration(statement) ||
      ts.isInterfaceDeclaration(statement) ||
      ts.isTypeAliasDeclaration(statement) ||
      ts.isEnumDeclaration(statement) ||
      ts.isModuleDeclaration(statement)
    ) {
      continue;
    }

    if (ts.isExportAssignment(statement)) continue;

    if (ts.isVariableStatement(statement)) {
      for (const decl of statement.declarationList.declarations) {
        if (!decl.initializer) continue;
        scanExpression(decl.initializer, (node) => report(node, `Top-level call in '${decl.name.getText(file)}'`));
      }
      continue;
    }

    if (ts.isExpressionStatement(statement)) {
      scanExpression(statement.expression, (node) => report(node, 'Top-level statement'));
      continue;
    }

    // Loops, ifs, try blocks and bare blocks at top level all execute on load.
    if (
      ts.isIfStatement(statement) ||
      ts.isIterationStatement(statement, false) ||
      ts.isTryStatement(statement) ||
      ts.isSwitchStatement(statement) ||
      ts.isBlock(statement)
    ) {
      report(statement, 'Top-level control flow');
    }
  }

  return issues;
};
