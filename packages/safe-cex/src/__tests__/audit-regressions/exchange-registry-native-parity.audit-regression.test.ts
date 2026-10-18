import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import ts from 'typescript';
import { describe, expect, it } from 'vitest';

type RegistryTarget = {
  exportName: string;
  modulePath: string;
};

const exchangesDirectory = resolve(process.cwd(), 'src/exchanges');

const propertyName = (name: ts.PropertyName, sourceFile: ts.SourceFile): string => {
  if (ts.isIdentifier(name) || ts.isStringLiteral(name) || ts.isNumericLiteral(name)) return name.text;
  return name.getText(sourceFile);
};

const registryObject = (sourceFile: ts.SourceFile): ts.ObjectLiteralExpression => {
  for (const statement of sourceFile.statements) {
    if (!ts.isVariableStatement(statement)) continue;

    for (const declaration of statement.declarationList.declarations) {
      if (
        ts.isIdentifier(declaration.name) &&
        declaration.name.text === 'exchangeRegistry' &&
        declaration.initializer &&
        ts.isObjectLiteralExpression(declaration.initializer)
      ) {
        return declaration.initializer;
      }
    }
  }

  throw new Error(`exchangeRegistry object not found in ${sourceFile.fileName}`);
};

const parseNativeTarget = (initializer: ts.Expression): RegistryTarget => {
  if (!ts.isCallExpression(initializer) || initializer.arguments.length !== 1) {
    throw new Error(`Expected native registry entry to call load(): ${initializer.getText()}`);
  }

  const loader = initializer.arguments[0];
  if (!ts.isArrowFunction(loader) || !ts.isPropertyAccessExpression(loader.body)) {
    throw new Error(`Expected native load() to return require(...).Export: ${initializer.getText()}`);
  }

  const requireCall = loader.body.expression;
  if (
    !ts.isCallExpression(requireCall) ||
    !ts.isIdentifier(requireCall.expression) ||
    requireCall.expression.text !== 'require' ||
    requireCall.arguments.length !== 1 ||
    !ts.isStringLiteral(requireCall.arguments[0])
  ) {
    throw new Error(`Expected native load() to use one static require(): ${initializer.getText()}`);
  }

  return {
    exportName: loader.body.name.text,
    modulePath: requireCall.arguments[0].text,
  };
};

const parseWebTarget = (initializer: ts.Expression): RegistryTarget => {
  if (!ts.isArrowFunction(initializer) || !ts.isCallExpression(initializer.body)) {
    throw new Error(`Expected web registry entry to be a dynamic-import loader: ${initializer.getText()}`);
  }

  const thenCall = initializer.body;
  if (
    !ts.isPropertyAccessExpression(thenCall.expression) ||
    thenCall.expression.name.text !== 'then' ||
    thenCall.arguments.length !== 1
  ) {
    throw new Error(`Expected web registry loader to select an export with .then(): ${initializer.getText()}`);
  }

  const importCall = thenCall.expression.expression;
  const selector = thenCall.arguments[0];
  if (
    !ts.isCallExpression(importCall) ||
    importCall.expression.kind !== ts.SyntaxKind.ImportKeyword ||
    importCall.arguments.length !== 1 ||
    !ts.isStringLiteral(importCall.arguments[0]) ||
    !ts.isArrowFunction(selector) ||
    !ts.isPropertyAccessExpression(selector.body)
  ) {
    throw new Error(`Expected web registry loader to use import(...).then(m => m.Export): ${initializer.getText()}`);
  }

  return {
    exportName: selector.body.name.text,
    modulePath: importCall.arguments[0].text,
  };
};

const parseRegistry = (
  fileName: 'registry.ts' | 'registry.native.ts',
  parseTarget: (initializer: ts.Expression) => RegistryTarget,
): Record<string, RegistryTarget> => {
  const path = resolve(exchangesDirectory, fileName);
  const sourceFile = ts.createSourceFile(path, readFileSync(path, 'utf8'), ts.ScriptTarget.Latest, true);

  return Object.fromEntries(
    registryObject(sourceFile).properties.map((property) => {
      if (!ts.isPropertyAssignment(property)) {
        throw new Error(`Expected a property assignment in ${fileName}: ${property.getText(sourceFile)}`);
      }
      return [propertyName(property.name, sourceFile), parseTarget(property.initializer)];
    }),
  );
};

const exportedClassNames = (modulePath: string): Set<string> => {
  const path = resolve(exchangesDirectory, `${modulePath}.ts`);
  const sourceFile = ts.createSourceFile(path, readFileSync(path, 'utf8'), ts.ScriptTarget.Latest, true);

  return new Set(
    sourceFile.statements.flatMap((statement) => {
      if (!ts.isClassDeclaration(statement) || !statement.name) return [];
      const isExported = statement.modifiers?.some((modifier) => modifier.kind === ts.SyntaxKind.ExportKeyword);
      return isExported ? [statement.name.text] : [];
    }),
  );
};

describe('React Native exchange registry parity', () => {
  // Parses every registered exchange module's AST; under CI load this outruns vitest's 5s default.
  it('loads the same module exports as the typed web registry', () => {
    const nativeRegistry = parseRegistry('registry.native.ts', parseNativeTarget);
    const webRegistry = parseRegistry('registry.ts', parseWebTarget);

    expect(nativeRegistry).toEqual(webRegistry);

    for (const [exchangeName, target] of Object.entries(nativeRegistry)) {
      expect(
        exportedClassNames(target.modulePath),
        `${exchangeName}: ${target.modulePath}.ts must export class ${target.exportName}`,
      ).toContain(target.exportName);
    }
  }, 30_000);
});
