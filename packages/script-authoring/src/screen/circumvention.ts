import type { CircumventionFinding, ScreenSeverity, ScreenTier } from './types';

import ts from 'typescript';

/**
 * Deterministic scan for attempts to reach around the module boundary.
 *
 * ## Why this is AST and not a regex
 *
 * Measured against the production corpus on 2026-08-07 (70 modules, 29
 * authors, aggregate counts only — no user code was read):
 *
 * - `createElement` appears in **28 of 70** modules. Every one of them is
 *   `React.createElement` from the JSX transpile, not `document.createElement`.
 * - `parent` appears in one module, as a local variable in a tree walk.
 *
 * A regex screen would hard-block 40% of the corpus on a name collision. The
 * scan therefore resolves whether an identifier is a **global** or a local
 * binding, and matches normalized member paths rather than bare words.
 *
 * ## Why the block list is short, and why there are three tiers
 *
 * Severity was set by measurement, not intuition. The same corpus scan found
 * `fetch` in 12 modules and `WebSocket` in 7 — both entirely ordinary today.
 * Because screening is fail-closed, a spurious `block` does not degrade a
 * feature, it stops an author saving their own work.
 *
 * But refusing everything real users do is its own failure, and superusers who
 * know what they are doing should not be neutered. Hence `standard`: reaching
 * the network or loading foreign code is allowed, in a frame with relaxed
 * network rules, and the module syncs and publishes like a strict one.
 *
 * What makes that safe is the opaque origin, which both tiers keep: the
 * exchange API keys never enter the frame, and they are the only asset the
 * threat model protects. Data leaving over the network, and third-party scripts
 * changing after screening, are accepted risks (owner decision, 2026-10-22).
 *
 * So `block` is reserved for evidence of intent: zero observed use, no
 * legitimate purpose, and mostly inert anyway.
 *
 * Re-run that measurement before promoting any rule to `block`. It has already
 * caught one bad call: `document.createElement('script'|'iframe')` was drafted
 * as a `block` and turned out to be in 3 of 70 modules, which would have locked
 * those authors out of saving permanently.
 *
 * ## Bias: false negatives over false positives
 *
 * Same posture as `checkCapabilities`. Scope resolution is deliberately
 * coarse — a name declared **anywhere** in the file suppresses the rule
 * everywhere in it. That over-suppresses (a local `parent` in one function
 * hides a global `parent` in another) and cannot over-fire. The model is the
 * layer that catches what this misses; there is no layer that un-breaks a
 * wrongly blocked save.
 */

interface Rule {
  rule: string;
  severity: ScreenSeverity;
  detail: string;
}

/**
 * Evidence of intent. Refused in EVERY tier, including standard.
 *
 * Zero observed corpus usage, no legitimate reason for a trading panel, and
 * mostly inert anyway under an opaque origin — which is the point: a module
 * reaching for a cookie it cannot read is telling you something about itself.
 */
const BLOCKED_PATHS: Record<string, Rule> = {
  'document.cookie': { rule: 'document-cookie', severity: 'block', detail: 'Reads or writes cookies.' },
  'document.write': { rule: 'document-write', severity: 'block', detail: 'Writes raw markup into the document.' },
  'document.writeln': { rule: 'document-write', severity: 'block', detail: 'Writes raw markup into the document.' },
  frameElement: { rule: 'frame-escape', severity: 'block', detail: 'Reaches for the containing frame element.' },
  opener: { rule: 'frame-escape', severity: 'block', detail: 'Reaches for the opening window.' },
  top: { rule: 'frame-escape', severity: 'block', detail: 'Reaches for the top-level window.' },
  'Object.setPrototypeOf': {
    rule: 'prototype-pollution',
    severity: 'block',
    detail: 'Rewrites an object prototype.',
  },
  'location.assign': { rule: 'navigation', severity: 'block', detail: 'Navigates the page away.' },
  'location.replace': { rule: 'navigation', severity: 'block', detail: 'Navigates the page away.' },
};

/**
 * Reaches the network, or loads code from somewhere else.
 *
 * Perfectly legitimate — and the reason there is a `standard` tier at all
 * rather than a wall. A module doing any of this mounts in the frame that
 * allows the network; it still syncs, commits and publishes.
 *
 * Counts are from the production corpus scan (2026-08-07).
 */
const STANDARD_PATHS: Record<string, Rule> = {
  // 12 of 70 modules.
  fetch: { rule: 'network-fetch', severity: 'standard', detail: 'Makes a network request.' },
  // 7 of 70.
  WebSocket: { rule: 'network-socket', severity: 'standard', detail: 'Opens a WebSocket.' },
  EventSource: { rule: 'network-sse', severity: 'standard', detail: 'Opens a server-sent event stream.' },
  XMLHttpRequest: { rule: 'xhr', severity: 'standard', detail: 'Opens an XMLHttpRequest.' },
  'navigator.sendBeacon': {
    rule: 'send-beacon',
    severity: 'standard',
    detail: 'Sends a beacon, which transmits data without needing a response.',
  },
  importScripts: { rule: 'import-scripts', severity: 'standard', detail: 'Loads and executes an external script.' },
};

/**
 * Recorded as evidence, never a gate on its own.
 *
 * Everything here either works fine inside the frame or is already dead by
 * virtue of the opaque origin. Note that the standard tier does NOT bring the
 * storage entries back to life — it relaxes the network, not the origin — so
 * those modules need `api.createStorageAtom` regardless of tier.
 */
const FLAGGED_PATHS: Record<string, Rule> = {
  // 3 of 70. Unreachable in either tier; flagged because the host's
  // localStorage is where the exchange API keys live, so an attempt is worth
  // seeing even though it cannot succeed.
  localStorage: { rule: 'local-storage', severity: 'flag', detail: 'Reads or writes localStorage.' },
  sessionStorage: { rule: 'session-storage', severity: 'flag', detail: 'Reads or writes sessionStorage.' },
  indexedDB: { rule: 'indexed-db', severity: 'flag', detail: 'Opens IndexedDB storage.' },
  // 1 of 70. Works in-frame: `'unsafe-eval'` is granted deliberately, since
  // transpiled module code runs through it.
  eval: { rule: 'eval', severity: 'flag', detail: 'Evaluates code from a string.' },
  parent: { rule: 'parent-window', severity: 'flag', detail: 'References the parent window.' },
  postMessage: { rule: 'post-message', severity: 'flag', detail: 'Posts a message across a frame boundary.' },
  atob: { rule: 'base64', severity: 'flag', detail: 'Decodes base64, which is often used to hide a string.' },
  btoa: { rule: 'base64', severity: 'flag', detail: 'Encodes base64.' },
  'navigator.clipboard': { rule: 'clipboard', severity: 'flag', detail: 'Reads or writes the clipboard.' },
  'document.createElement': {
    rule: 'create-element',
    severity: 'flag',
    detail: 'Creates a DOM element directly rather than rendering it.',
  },
};

/**
 * Property names that are the finding regardless of what they hang off.
 *
 * Built with `fromEntries` and NOT an object literal. In a literal,
 * `__proto__: value` invokes the prototype setter instead of creating an own
 * property — the table would be `{}` with a rule hidden on its prototype, so
 * an own-property lookup finds nothing and this `block` rule silently stops
 * firing. `fromEntries` defines a real own property. Verified by the
 * "blocks prototype pollution" test, which caught exactly that regression.
 */
const BLOCKED_PROPERTY_WRITES: Record<string, Rule> = Object.fromEntries([
  ['__proto__', { rule: 'prototype-pollution', severity: 'block', detail: 'Assigns through __proto__.' }],
]);

const FLAGGED_PROPERTIES: Record<string, Rule> = {
  innerHTML: { rule: 'html-injection', severity: 'flag', detail: 'Assigns raw HTML.' },
  outerHTML: { rule: 'html-injection', severity: 'flag', detail: 'Assigns raw HTML.' },
  insertAdjacentHTML: { rule: 'html-injection', severity: 'flag', detail: 'Inserts raw HTML.' },
  dangerouslySetInnerHTML: { rule: 'html-injection', severity: 'flag', detail: 'Renders raw HTML.' },
};

/** Element types that execute or embed once created. */
const EXECUTABLE_ELEMENTS = new Set(['script', 'iframe', 'object', 'embed']);

/** Host objects that are transparent when they lead a member chain. */
const GLOBAL_ROOTS = new Set(['window', 'globalThis', 'self']);

const MAX_SNIPPET = 120;

/**
 * Every name bound anywhere in the file.
 *
 * Collected across all scopes on purpose — see the false-negative bias above.
 * Includes imports, since a module that imports its own `parent` helper must
 * not trip the global rule.
 */
const collectDeclaredNames = (source: ts.SourceFile): Set<string> => {
  const names = new Set<string>();

  const addBindingName = (name: ts.BindingName | undefined): void => {
    if (!name) return;
    if (ts.isIdentifier(name)) {
      names.add(name.text);
      return;
    }
    // Destructuring: `const { parent } = x` and `const [parent] = y` both bind.
    for (const element of name.elements) {
      if (ts.isBindingElement(element)) addBindingName(element.name);
    }
  };

  const visit = (node: ts.Node): void => {
    if (ts.isVariableDeclaration(node) || ts.isParameter(node)) {
      addBindingName(node.name);
    } else if (
      (ts.isFunctionDeclaration(node) || ts.isClassDeclaration(node) || ts.isFunctionExpression(node)) &&
      node.name
    ) {
      names.add(node.name.text);
    } else if (ts.isImportSpecifier(node) || ts.isImportClause(node) || ts.isNamespaceImport(node)) {
      if (node.name && ts.isIdentifier(node.name)) names.add(node.name.text);
    } else if (ts.isBindingElement(node)) {
      addBindingName(node.name);
    }
    ts.forEachChild(node, visit);
  };

  visit(source);
  return names;
};

/**
 * Resolve a member expression to a dotted path, or null when any segment is
 * dynamic or the root is a local binding.
 *
 * `window.document.cookie` and `document.cookie` both normalize to
 * `document.cookie`, so a rule is written once.
 */
const globalPath = (node: ts.Node, declared: Set<string>): string | null => {
  const segments: string[] = [];
  let current: ts.Node = node;

  for (;;) {
    if (ts.isPropertyAccessExpression(current)) {
      segments.unshift(current.name.text);
      current = current.expression;
      continue;
    }
    if (ts.isElementAccessExpression(current)) {
      const argument = current.argumentExpression;
      // A computed key we cannot read is exactly the obfuscation case; give up
      // rather than guess, and let the model see the raw source.
      if (!argument || !ts.isStringLiteralLike(argument)) return null;
      segments.unshift(argument.text);
      current = current.expression;
      continue;
    }
    break;
  }

  if (!ts.isIdentifier(current)) return null;
  const root = current.text;
  // A locally declared root is never the global — this is the rule that keeps
  // `React.createElement` and a local `parent` from firing.
  if (declared.has(root)) return null;

  if (GLOBAL_ROOTS.has(root)) {
    if (segments.length === 0) return null;
    return segments.join('.');
  }
  return [root, ...segments].join('.');
};

/**
 * Look a path up in the three rule tables WITHOUT inheriting from
 * `Object.prototype`.
 *
 * A plain `TABLE[path]` is a prototype-chain lookup, so `toString`,
 * `constructor`, `valueOf` and `hasOwnProperty` all resolve to a truthy
 * function — and spreading a function into a finding yields one with no
 * `rule`, no `severity` and no `detail`. Measured on the production corpus:
 * 10 of 70 modules produced these.
 *
 * Tiering was unaffected, because `requiredTier` and `hasBlockingFinding` both
 * test for a specific severity and `undefined` is not one. The damage was
 * upstream of that: the junk findings are fed to the model in
 * `buildScreenPrompt`, and screening is fail-closed, so noise that reads as
 * evidence of tampering can only push a verdict toward refusing a module its
 * author is simply trying to save.
 *
 * Same failure mode the mirror's slice-name validation exists to prevent — see
 * `module-sandbox-policy/CLAUDE.md`. An unchecked name is a property lookup.
 */
const ownRule = (table: Record<string, Rule>, key: string): Rule | undefined =>
  Object.prototype.hasOwnProperty.call(table, key) ? table[key] : undefined;

const lookupRule = (path: string): Rule | undefined =>
  ownRule(BLOCKED_PATHS, path) ?? ownRule(STANDARD_PATHS, path) ?? ownRule(FLAGGED_PATHS, path);

const snippetOf = (node: ts.Node, source: ts.SourceFile): string => {
  const text = node.getText(source).replace(/\s+/g, ' ').trim();
  return text.length > MAX_SNIPPET ? `${text.slice(0, MAX_SNIPPET)}…` : text;
};

const lineOf = (node: ts.Node, source: ts.SourceFile): number =>
  ts.getLineAndCharacterOfPosition(source, node.getStart(source)).line + 1;

/**
 * Scan a module for boundary-circumvention signals.
 *
 * Returns findings in source order. A parse failure returns no findings: a
 * file that does not parse cannot run either, and the typecheck pass is what
 * should report it. Speculative findings stacked on a syntax error bury the
 * one thing that actually needs fixing — same rule as `validate()`.
 */
export const scanCircumvention = (code: string): CircumventionFinding[] => {
  const source = ts.createSourceFile('module.tsx', code, ts.ScriptTarget.ES2020, true, ts.ScriptKind.TSX);
  if ((source as unknown as { parseDiagnostics?: unknown[] }).parseDiagnostics?.length) return [];

  const declared = collectDeclaredNames(source);
  const findings: CircumventionFinding[] = [];
  const push = (rule: Rule, node: ts.Node): void => {
    findings.push({ ...rule, line: lineOf(node, source), snippet: snippetOf(node, source) });
  };

  const visit = (node: ts.Node): void => {
    // Dynamic import() loads and runs code from elsewhere — the same category
    // as pulling a script off a CDN, so it earns the standard tier rather than a
    // refusal. Zero corpus usage either way, so this is a consistency call.
    if (ts.isCallExpression(node) && node.expression.kind === ts.SyntaxKind.ImportKeyword) {
      push({ rule: 'dynamic-import', severity: 'standard', detail: 'Loads another module at runtime.' }, node);
    }

    // `new Function(...)` builds executable code from strings.
    if (ts.isNewExpression(node) && ts.isIdentifier(node.expression) && node.expression.text === 'Function') {
      if (!declared.has('Function')) {
        push({ rule: 'function-constructor', severity: 'flag', detail: 'Builds a function from a string.' }, node);
      }
    }

    // Assigning to window.location / location.href navigates away.
    if (ts.isBinaryExpression(node) && node.operatorToken.kind === ts.SyntaxKind.EqualsToken) {
      const target = globalPath(node.left, declared);
      if (target === 'location' || target === 'location.href') {
        push({ rule: 'navigation', severity: 'block', detail: 'Navigates the page away.' }, node);
      }
      if (ts.isPropertyAccessExpression(node.left)) {
        const written = ownRule(BLOCKED_PROPERTY_WRITES, node.left.name.text);
        if (written) push(written, node);
      }
    }

    // document.createElement('script') — the argument decides how bad it is.
    if (ts.isCallExpression(node)) {
      const callee = globalPath(node.expression, declared);
      if (callee === 'document.createElement') {
        const [first] = node.arguments;
        if (first && ts.isStringLiteralLike(first) && EXECUTABLE_ELEMENTS.has(first.text.toLowerCase())) {
          // Measured: 3 of 70 production modules do this (embedding a widget or
          // pulling a charting library). The code that runs is then not the
          // code that was screened — an accepted risk the standard tier carries.
          push(
            {
              rule: 'create-executable-element',
              severity: 'standard',
              detail: `Creates a <${first.text.toLowerCase()}> element, which executes or embeds external content.`,
            },
            node,
          );
        }
      }
    }

    if (ts.isPropertyAccessExpression(node) || ts.isElementAccessExpression(node) || ts.isIdentifier(node)) {
      // Skip the `.name` half of a member expression; it is not a reference.
      const isMemberName = node.parent && ts.isPropertyAccessExpression(node.parent) && node.parent.name === node;
      // Skip object-literal keys and JSX attribute names.
      const isPropertyKey =
        node.parent &&
        ((ts.isPropertyAssignment(node.parent) && node.parent.name === node) ||
          ts.isJsxAttribute(node.parent) ||
          ts.isPropertySignature(node.parent));

      if (!isMemberName && !isPropertyKey) {
        const path = globalPath(node, declared);
        if (path) {
          const matched = lookupRule(path);
          // Only report the outermost node of a chain, so `document.cookie`
          // does not also report `document`.
          const parentPath = node.parent ? globalPath(node.parent, declared) : null;
          if (matched && parentPath !== path) push(matched, node);
        }
      }

      if (ts.isPropertyAccessExpression(node)) {
        const flagged = ownRule(FLAGGED_PROPERTIES, node.name.text);
        if (flagged) push(flagged, node);
      }
    }

    // dangerouslySetInnerHTML as a JSX attribute.
    if (ts.isJsxAttribute(node) && ts.isIdentifier(node.name)) {
      const flagged = ownRule(FLAGGED_PROPERTIES, node.name.text);
      if (flagged) push(flagged, node);
    }

    ts.forEachChild(node, visit);
  };

  visit(source);
  return findings.sort((a, b) => a.line - b.line);
};

/** True when anything found is fatal on its own, in any tier. */
export const hasBlockingFinding = (findings: CircumventionFinding[]): boolean =>
  findings.some((finding) => finding.severity === 'block');

/**
 * The most restrictive tier the findings demand.
 *
 * `blocked` wins over `standard` wins over `strict`. Note this is what the
 * STATIC scan alone requires — the model can raise it further but never lower
 * it, which is the ordering invariant enforced in `screenModule`.
 */
export const requiredTier = (findings: CircumventionFinding[]): ScreenTier => {
  if (findings.some((f) => f.severity === 'block')) return 'blocked';
  if (findings.some((f) => f.severity === 'standard')) return 'standard';
  return 'strict';
};
