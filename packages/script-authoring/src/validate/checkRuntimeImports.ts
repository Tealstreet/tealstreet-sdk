import ts from 'typescript';

import type { ValidationIssue } from './validateScript';

/**
 * Reject imports that typecheck but die at runtime.
 *
 * The typechecker is satisfied by the ambient `declare module` blocks in the
 * generated types bundle, so
 *
 * ```ts
 * import { OrderSide } from '@tealstreet/script-api-core';
 * ```
 *
 * validates clean. At runtime it does not: `scripts-store.ts` loads scripts
 * with `import(fileURL)` from `~/.tealstreet/scripts/`, a plain directory with
 * no `node_modules`, so the specifier cannot resolve and the whole script
 * fails to load.
 *
 * This is not a theoretical hazard — it is the single most likely thing an AI
 * generation loop emits, because value imports are what every other TypeScript
 * codebase looks like. Catching it here turns a confusing runtime failure into
 * a repairable diagnostic.
 *
 * `import type { … }` and per-specifier `import { type X }` are both fine:
 * they are erased before the file is ever loaded.
 */
export const checkRuntimeImports = (source: string): ValidationIssue[] => {
  const file = ts.createSourceFile('/script.ts', source, ts.ScriptTarget.ESNext, true);
  const issues: ValidationIssue[] = [];

  const positionOf = (node: ts.Node): Pick<ValidationIssue, 'line' | 'column'> => {
    const { line, character } = file.getLineAndCharacterOfPosition(node.getStart(file));
    return { line: line + 1, column: character + 1 };
  };

  // Dynamic `import('…')` resolves at runtime from a directory with no
  // node_modules, so it fails exactly like a static value import — and being
  // an expression it can appear anywhere, hence the full walk.
  const visit = (node: ts.Node): void => {
    if (ts.isCallExpression(node) && node.expression.kind === ts.SyntaxKind.ImportKeyword) {
      const arg = node.arguments[0];
      const target = arg && ts.isStringLiteral(arg) ? `'${arg.text}'` : 'a computed specifier';
      issues.push({
        message: `Dynamic import of ${target} will fail at runtime: scripts load from a directory with no node_modules.`,
        ...positionOf(node),
      });
    }
    ts.forEachChild(node, visit);
  };
  visit(file);

  for (const statement of file.statements) {
    // `export { X } from '…'` and `export * from '…'` are re-exports: the
    // module specifier survives erasure exactly like an import.
    if (ts.isExportDeclaration(statement) && statement.moduleSpecifier) {
      if (!statement.isTypeOnly) {
        const spec = ts.isStringLiteral(statement.moduleSpecifier) ? statement.moduleSpecifier.text : '<unknown>';
        const clause = statement.exportClause;
        const allTypeOnly =
          clause !== undefined && ts.isNamedExports(clause) && clause.elements.every((el) => el.isTypeOnly);
        if (!allTypeOnly) {
          issues.push({
            message: `Re-export from '${spec}' will fail at runtime: scripts load from a directory with no node_modules. A script exports only its default function.`,
            ...positionOf(statement),
          });
        }
      }
      continue;
    }
    if (!ts.isImportDeclaration(statement)) continue;
    const specifier = ts.isStringLiteral(statement.moduleSpecifier) ? statement.moduleSpecifier.text : '<unknown>';
    const clause = statement.importClause;

    // `import '@tealstreet/…'` — a bare side-effect import is a value import
    // with nothing to erase, so it always survives to runtime.
    if (!clause) {
      issues.push({
        message: `Side-effect import of '${specifier}' will fail at runtime: scripts load from a directory with no node_modules. Remove it.`,
        ...positionOf(statement),
      });
      continue;
    }

    if (clause.isTypeOnly) continue;

    if (clause.name) {
      issues.push({
        message: `Default import from '${specifier}' will fail at runtime: scripts load from a directory with no node_modules. Use 'import type' — the script API is passed in as the 'api' parameter, not imported.`,
        ...positionOf(clause.name),
      });
    }

    const bindings = clause.namedBindings;
    if (bindings && ts.isNamespaceImport(bindings)) {
      issues.push({
        message: `Namespace import from '${specifier}' will fail at runtime: scripts load from a directory with no node_modules.`,
        ...positionOf(bindings),
      });
    } else if (bindings && ts.isNamedImports(bindings)) {
      for (const element of bindings.elements) {
        if (element.isTypeOnly) continue;
        issues.push({
          message: `Value import '${element.name.text}' from '${specifier}' will fail at runtime: scripts load from a directory with no node_modules. Use 'import type' for types; runtime values come from the 'api' parameter (e.g. api.constants.OrderSide).`,
          ...positionOf(element),
        });
      }
    }
  }

  return issues;
};
