import type { ApiPath } from '@tealstreet/script-api-core';
import { getApiPathAvailability, isKnownApiPath, type TargetName } from '@tealstreet/script-api-core';
import ts from 'typescript';

import { readScriptMeta } from './checkCapabilities';
import type { ValidationIssue } from './validateScript';

const findMetaObject = (file: ts.SourceFile): ts.ObjectLiteralExpression | undefined => {
  for (const statement of file.statements) {
    if (!ts.isVariableStatement(statement)) continue;
    const isExported = statement.modifiers?.some((m) => m.kind === ts.SyntaxKind.ExportKeyword);
    if (!isExported) continue;
    for (const decl of statement.declarationList.declarations) {
      if (!ts.isIdentifier(decl.name) || decl.name.text !== 'meta') continue;
      return decl.initializer && ts.isObjectLiteralExpression(decl.initializer) ? decl.initializer : undefined;
    }
  }
  return undefined;
};

const getProperty = (obj: ts.ObjectLiteralExpression, name: string): ts.Expression | undefined => {
  for (const prop of obj.properties) {
    if (!ts.isPropertyAssignment(prop)) continue;
    const key = prop.name;
    if (ts.isIdentifier(key) && key.text === name) return prop.initializer;
    if (ts.isStringLiteral(key) && key.text === name) return prop.initializer;
  }
  return undefined;
};

const literalTargets = (file: ts.SourceFile): readonly TargetName[] | undefined => {
  const meta = findMetaObject(file);
  const node = meta ? getProperty(meta, 'targets') : undefined;
  if (!node || !ts.isArrayLiteralExpression(node)) return undefined;
  const targets: TargetName[] = [];
  for (const element of node.elements) {
    if (!ts.isStringLiteral(element)) return undefined;
    targets.push(element.text as TargetName);
  }
  return targets;
};

const findApiAnchor = (file: ts.SourceFile): { apiName: string } | undefined => {
  for (const statement of file.statements) {
    let fn: ts.SignatureDeclarationBase | undefined;
    if (ts.isFunctionDeclaration(statement)) {
      const isDefaultExport = statement.modifiers?.some((m) => m.kind === ts.SyntaxKind.DefaultKeyword);
      if (isDefaultExport) fn = statement;
    } else if (ts.isExportAssignment(statement)) {
      const expr = statement.expression;
      if (ts.isArrowFunction(expr) || ts.isFunctionExpression(expr)) fn = expr;
    }
    if (!fn) continue;
    const param = fn.parameters[1];
    const body = (fn as ts.FunctionLikeDeclaration).body;
    if (param && ts.isIdentifier(param.name) && body) return { apiName: param.name.text };
  }
  return undefined;
};

const rebindsName = (node: ts.Node, name: string): boolean => {
  if (ts.isFunctionLike(node)) {
    return node.parameters.some((p) => ts.isIdentifier(p.name) && p.name.text === name);
  }
  if (ts.isVariableDeclaration(node)) {
    return ts.isIdentifier(node.name) && node.name.text === name;
  }
  return false;
};

const propertyPath = (node: ts.Expression, aliases: ReadonlyMap<string, ApiPath>, apiName: string): string | undefined => {
  if (ts.isIdentifier(node)) {
    if (node.text === apiName) return 'api';
    return aliases.get(node.text);
  }
  if (ts.isPropertyAccessExpression(node)) {
    const base = propertyPath(node.expression, aliases, apiName);
    return base ? `${base}.${node.name.text}` : undefined;
  }
  if (ts.isElementAccessExpression(node) && ts.isStringLiteralLike(node.argumentExpression)) {
    const base = propertyPath(node.expression, aliases, apiName);
    return base ? `${base}.${node.argumentExpression.text}` : undefined;
  }
  return undefined;
};

const nearestKnownPath = (path: string): ApiPath | undefined => {
  const parts = path.split('.');
  while (parts.length >= 2) {
    const candidate = parts.join('.');
    if (isKnownApiPath(candidate)) return candidate;
    parts.pop();
  }
  return undefined;
};

const addBindingAliases = (
  pattern: ts.BindingName,
  basePath: string,
  aliases: Map<string, ApiPath>,
): void => {
  if (ts.isIdentifier(pattern)) {
    const path = nearestKnownPath(basePath);
    if (path) aliases.set(pattern.text, path);
    return;
  }
  for (const element of pattern.elements) {
    if (ts.isOmittedExpression(element)) continue;
    const propertyName = element.propertyName;
    const key = propertyName && ts.isIdentifier(propertyName) ? propertyName.text : undefined;
    const bindingName = element.name;
    if (ts.isIdentifier(bindingName)) {
      const path = nearestKnownPath(`${basePath}.${key ?? bindingName.text}`);
      if (path) aliases.set(bindingName.text, path);
    } else {
      addBindingAliases(bindingName, `${basePath}.${key ?? ''}`, aliases);
    }
  }
};

const collectApiPaths = (root: ts.Node, apiName: string): Map<ApiPath, ts.Node> => {
  const found = new Map<ApiPath, ts.Node>();
  const aliases = new Map<string, ApiPath>();

  const addPath = (path: string | undefined, node: ts.Node): void => {
    if (!path) return;
    const known = nearestKnownPath(path);
    if (known && !found.has(known)) found.set(known, node);
  };

  const visit = (node: ts.Node): void => {
    if (ts.isVariableDeclaration(node) && node.initializer) {
      const base = propertyPath(node.initializer, aliases, apiName);
      if (base) addBindingAliases(node.name, base, aliases);
    }

    if (ts.isPropertyAccessExpression(node) || ts.isElementAccessExpression(node)) {
      addPath(propertyPath(node, aliases, apiName), node);
    } else if (ts.isCallExpression(node) && ts.isIdentifier(node.expression)) {
      addPath(aliases.get(node.expression.text), node.expression);
    }

    ts.forEachChild(node, (child) => {
      if (ts.isIdentifier(child) && aliases.has(child.text)) return;
      if (!ts.isFunctionLike(child) && rebindsName(child, apiName)) return;
      visit(child);
    });
  };

  visit(root);
  for (const path of [...found.keys()]) {
    if ([...found.keys()].some((other) => other !== path && other.startsWith(`${path}.`))) {
      found.delete(path);
    }
  }
  return found;
};

const positionOf = (file: ts.SourceFile, node: ts.Node): Pick<ValidationIssue, 'line' | 'column'> => {
  const { line, character } = file.getLineAndCharacterOfPosition(node.getStart(file));
  return { line: line + 1, column: character + 1 };
};

export const checkTargetSurface = (source: string): ValidationIssue[] => {
  const file = ts.createSourceFile('/script.ts', source, ts.ScriptTarget.ESNext, true);
  const targets = readScriptMeta(source)?.targets ?? literalTargets(file);
  if (!targets || targets.length === 0) return [];

  const anchor = findApiAnchor(file);
  if (!anchor) return [];

  const issues: ValidationIssue[] = [];
  for (const [path, node] of collectApiPaths(file, anchor.apiName)) {
    const availability = getApiPathAvailability(path, targets);
    if (availability.allowed) continue;
    const targetText = availability.excludedBy.map((target) => `'${target}'`).join(', ');
    const alternatives = availability.alternatives.length
      ? ` Use ${availability.alternatives.join(' or ')} instead, or remove the excluding target.`
      : ' Remove the call or remove the excluding target.';
    issues.push({
      message: `${path} is not available for target ${targetText}; this script targets [${targets.join(', ')}].${alternatives}`,
      ...positionOf(file, node),
    });
  }

  return issues;
};
