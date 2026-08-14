import type { ReadMethodName, TargetName, WriteMethodName } from '@tealstreet/script-api-core';

import ts from 'typescript';

import { IMPLIED_WRITES, isReadMethod, isWriteMethod } from './capabilityNames';
import type { ValidationIssue } from './validateScript';

/**
 * Authoring-time sibling of `script-api-core/src/runtime/capabilityCheck.ts`.
 * The runtime version throws at dispatch; this one reports before the script
 * ever runs, so an AI repair loop can fix the declaration instead of the user
 * discovering it mid-trade.
 *
 * Its second job is pedagogical: feeding a capability error back into the
 * repair prompt teaches the model the capability model with no prompt
 * engineering about capabilities at all.
 *
 * ## Posture: false negatives are fine, false positives are not
 *
 * This check drives an automated repair loop. A spurious "capability missing"
 * sends the model in circles editing a correct declaration. So every
 * ambiguity resolves to **silence**:
 *
 * - `meta` that is not a plain statically-analyzable object literal → no
 *   findings at all. Scripts may legally build `meta` dynamically
 *   (`scripts-store.ts` only requires an object at import time).
 * - Aliasing (`const ex = api.exchange`), destructuring, and element access
 *   (`api['exchange']['placeOrder']`) are not chased.
 *
 * The `api` root is anchored to the **second parameter of the default-exported
 * function**, which is where the runtime binds it. That kills the
 * shadowed-local-`api` false positive.
 */

export interface DeclaredCapabilities {
  targets?: TargetName[];
  writes: Set<string>;
  /** `undefined` means "all reads permitted" — SPEC §4.1's author-only default. */
  reads?: Set<string>;
  /**
   * Account allowlist. An EMPTY array means "no restriction" (SPEC §4.1), not
   * "no accounts" — reading it the other way inverts the meaning of the most
   * consequential field here. `undefined` means it was not statically readable.
   */
  accounts?: string[];
  runCommandEnabled: boolean;
}

const findMetaObject = (file: ts.SourceFile): ts.ObjectLiteralExpression | undefined => {
  for (const statement of file.statements) {
    if (!ts.isVariableStatement(statement)) continue;
    const isExported = statement.modifiers?.some((m) => m.kind === ts.SyntaxKind.ExportKeyword);
    if (!isExported) continue;
    for (const decl of statement.declarationList.declarations) {
      if (!ts.isIdentifier(decl.name) || decl.name.text !== 'meta') continue;
      if (decl.initializer && ts.isObjectLiteralExpression(decl.initializer)) return decl.initializer;
      // Declared but not a literal → not analyzable.
      return undefined;
    }
  }
  return undefined;
};

const getProperty = (obj: ts.ObjectLiteralExpression, name: string): ts.Expression | undefined => {
  for (const prop of obj.properties) {
    // A spread means we cannot see the whole picture; refuse to guess.
    if (ts.isSpreadAssignment(prop)) return undefined;
    if (!ts.isPropertyAssignment(prop)) continue;
    const key = prop.name;
    if (ts.isIdentifier(key) && key.text === name) return prop.initializer;
    if (ts.isStringLiteral(key) && key.text === name) return prop.initializer;
  }
  return undefined;
};

/** String-literal array → Set, or `undefined` when anything is not a literal. */
const literalStringSet = (node: ts.Expression | undefined): Set<string> | undefined => {
  if (!node || !ts.isArrayLiteralExpression(node)) return undefined;
  const out = new Set<string>();
  for (const element of node.elements) {
    if (!ts.isStringLiteral(element)) return undefined;
    out.add(element.text);
  }
  return out;
};

const hasUnanalyzableShape = (obj: ts.ObjectLiteralExpression): boolean =>
  obj.properties.some(
    (p) =>
      ts.isSpreadAssignment(p) ||
      ts.isShorthandPropertyAssignment(p) ||
      (ts.isPropertyAssignment(p) && ts.isComputedPropertyName(p.name)),
  );

const readDeclaredCapabilities = (file: ts.SourceFile): DeclaredCapabilities | undefined => {
  const meta = findMetaObject(file);
  if (!meta || hasUnanalyzableShape(meta)) return undefined;

  const caps = getProperty(meta, 'capabilities');
  if (!caps || !ts.isObjectLiteralExpression(caps) || hasUnanalyzableShape(caps)) return undefined;

  const targetsNode = getProperty(meta, 'targets');
  const targetSet = targetsNode === undefined ? undefined : literalStringSet(targetsNode);
  if (targetsNode !== undefined && targetSet === undefined) return undefined;

  const writes = literalStringSet(getProperty(caps, 'writes'));
  if (!writes) return undefined;

  const readsNode = getProperty(caps, 'reads');
  const reads = readsNode === undefined ? undefined : literalStringSet(readsNode);
  // `reads` present but not statically readable → bail rather than guess.
  if (readsNode !== undefined && reads === undefined) return undefined;

  // Omitted runCommand means disabled (SPEC §4.1). But a runCommand block we
  // cannot fully read must NOT be treated as disabled — `runCommand: RC` or
  // `{ ...base, enabled: true }` are legitimate and would otherwise be flagged
  // as "not enabled", exactly the false positive this file forbids.
  const accountsNode = getProperty(caps, 'accounts');
  const accountsSet = accountsNode === undefined ? undefined : literalStringSet(accountsNode);
  // Present but unreadable → bail rather than describe it wrongly downstream.
  if (accountsNode !== undefined && accountsSet === undefined) return undefined;

  const runCommand = getProperty(caps, 'runCommand');
  let runCommandEnabled = false;
  if (runCommand !== undefined) {
    if (!ts.isObjectLiteralExpression(runCommand) || hasUnanalyzableShape(runCommand)) return undefined;
    const enabled = getProperty(runCommand, 'enabled');
    if (enabled === undefined) return undefined;
    if (enabled.kind === ts.SyntaxKind.TrueKeyword) runCommandEnabled = true;
    else if (enabled.kind !== ts.SyntaxKind.FalseKeyword) return undefined;
  }

  return {
    targets: targetSet ? ([...targetSet] as TargetName[]) : undefined,
    writes,
    reads,
    accounts: accountsSet ? [...accountsSet] : undefined,
    runCommandEnabled,
  };
};

/**
 * The default-exported function and the identifier it binds `api` to (its 2nd
 * parameter, matching how the runtime invokes it). Returning the function too
 * lets the walk start at its BODY — walking the whole file would have to skip
 * the anchor function itself as an `api` rebinder.
 */
const findApiAnchor = (file: ts.SourceFile): { body: ts.Node; apiName: string } | undefined => {
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
    if (param && ts.isIdentifier(param.name) && body) {
      return { body, apiName: param.name.text };
    }
  }
  return undefined;
};

interface UsedCapabilities {
  writes: Map<WriteMethodName, ts.Node>;
  reads: Map<ReadMethodName, ts.Node>;
  runCommandNode?: ts.Node;
}

/**
 * Whether `node` introduces its own binding for `apiName`, in which case its
 * whole subtree refers to something else. Matching by identifier text alone
 * flags a local mock or helper named `api` — a false finding, and this check
 * feeds an automated repair loop.
 */
const rebindsApi = (node: ts.Node, apiName: string): boolean => {
  if (ts.isFunctionLike(node)) {
    return node.parameters.some((p) => ts.isIdentifier(p.name) && p.name.text === apiName);
  }
  if (ts.isBlock(node)) {
    return node.statements.some(
      (st) =>
        ts.isVariableStatement(st) &&
        st.declarationList.declarations.some((d) => ts.isIdentifier(d.name) && d.name.text === apiName),
    );
  }
  return false;
};

const collectUsage = (root: ts.Node, apiName: string): UsedCapabilities => {
  const used: UsedCapabilities = { writes: new Map(), reads: new Map() };

  const visit = (node: ts.Node): void => {
    if (ts.isCallExpression(node) && ts.isPropertyAccessExpression(node.expression)) {
      const access = node.expression;
      const method = access.name.text;
      const target = access.expression;

      // api.exchange.<write>(…)
      if (
        ts.isPropertyAccessExpression(target) &&
        target.name.text === 'exchange' &&
        ts.isIdentifier(target.expression) &&
        target.expression.text === apiName
      ) {
        if (isWriteMethod(method) && !used.writes.has(method)) used.writes.set(method, access.name);
        if (method === 'runCommand') used.runCommandNode = access.name;
      }

      // api.<read>(…)
      if (ts.isIdentifier(target) && target.text === apiName && isReadMethod(method) && !used.reads.has(method)) {
        used.reads.set(method, access.name);
      }
    }
    ts.forEachChild(node, (child) => {
      if (rebindsApi(child, apiName)) return;
      visit(child);
    });
  };

  visit(root);
  return used;
};

/**
 * Read a script's declared capabilities out of its source.
 *
 * Exported because consumers (the MCP install/consent prompt, the web
 * capability-review UI) need to show a user what a script may do. Reading it
 * from the code rather than from a sidecar object means the disclosure and the
 * thing the runtime enforces cannot drift — they are the same declaration.
 *
 * Returns `undefined` when `meta` is not statically analyzable; callers must
 * treat that as "unknown", never as "no capabilities".
 */
export const readScriptMeta = (source: string): DeclaredCapabilities | undefined =>
  readDeclaredCapabilities(ts.createSourceFile('/script.ts', source, ts.ScriptTarget.ESNext, true));

export const checkCapabilities = (source: string): ValidationIssue[] => {
  const file = ts.createSourceFile('/script.ts', source, ts.ScriptTarget.ESNext, true);

  const declared = readDeclaredCapabilities(file);
  if (!declared) return [];

  const anchor = findApiAnchor(file);
  if (!anchor) return [];

  const used = collectUsage(anchor.body, anchor.apiName);
  const issues: ValidationIssue[] = [];

  const positionOf = (node: ts.Node): Pick<ValidationIssue, 'line' | 'column'> => {
    const { line, character } = file.getLineAndCharacterOfPosition(node.getStart(file));
    return { line: line + 1, column: character + 1 };
  };

  const runCommandIssueFires = used.runCommandNode !== undefined && !declared.runCommandEnabled;

  for (const [method, node] of used.writes) {
    // The dedicated runCommand message is strictly more actionable than
    // "missing write 'runCommand'"; emitting both just adds noise.
    if (method === 'runCommand' && runCommandIssueFires) continue;
    const required: string[] = [method, ...(IMPLIED_WRITES[method] ?? [])];
    const missing = required.filter((r) => !declared.writes.has(r));
    if (missing.length === 0) continue;
    const detail =
      missing.length === 1 && missing[0] === method
        ? `'${method}'`
        : `${missing.map((m) => `'${m}'`).join(' and ')} (api.exchange.${method} needs both — the runtime synthesizes it via placeOrder)`;
    issues.push({
      message: `Script calls api.exchange.${method}() but meta.capabilities.writes is missing ${detail}. Add it, or remove the call.`,
      ...positionOf(node),
    });
  }

  if (declared.reads) {
    for (const [method, node] of used.reads) {
      if (declared.reads.has(method)) continue;
      issues.push({
        message: `Script calls api.${method}() but meta.capabilities.reads does not include '${method}'. Add it, or omit 'reads' entirely to allow all reads.`,
        ...positionOf(node),
      });
    }
  }

  if (runCommandIssueFires && used.runCommandNode) {
    issues.push({
      message: `Script calls api.exchange.runCommand() but meta.capabilities.runCommand is not enabled. Set runCommand: { enabled: true, allowedVerbs: [...] }.`,
      ...positionOf(used.runCommandNode),
    });
  }

  return issues;
};
