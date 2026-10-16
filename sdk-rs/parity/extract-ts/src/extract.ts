import { existsSync, readdirSync } from 'node:fs';
import { join, relative, resolve, sep } from 'node:path';
import ts from 'typescript';

// Add phase scope here; manifest prefixes must remain disjoint from the legacy root.
export const sourceRoots = [
  { ts: 'packages/safe-cex/src/', manifest: '', package: 'packages/safe-cex/' },
  { ts: 'packages/http-over-websocket/src/', manifest: 'http-over-websocket/', package: 'packages/http-over-websocket/' },
  { ts: 'packages/script-api-core/src/order/', manifest: 'script-api-core/order/', package: 'packages/script-api-core/' },
  { ts: 'packages/safe-cex-web/src/candleCache/', manifest: 'safe-cex-web/candleCache/', package: 'packages/safe-cex-web/' },
  { ts: 'packages/safe-cex-server/src/candleCache/', manifest: 'safe-cex-server/candleCache/', package: 'packages/safe-cex-server/' },
  { ts: 'packages/safe-cex-react-native/src/candleCache/', manifest: 'safe-cex-react-native/candleCache/', package: 'packages/safe-cex-react-native/' },
  // Lighter's complete SDK dependency closure: signer primitives plus venue-owned
  // client composition, protocol types/constants, utilities and the default API.
  { ts: 'packages/poseidon-crypto/src/', manifest: 'poseidon-crypto/', package: 'packages/poseidon-crypto/' },
  { ts: 'packages/lighter-sdk/src/', manifest: 'lighter-sdk/', package: 'packages/lighter-sdk/' },
  { ts: 'packages/json-parse/src/', manifest: 'json-parse/', package: 'packages/json-parse/' },
] as const;
export const sourcePrefix = sourceRoots[0].ts;

export function manifestPathForSource(source: string): string {
  if (source.split('/').some((part) => !part || part === '.' || part === '..') || source.includes('\\')) {
    throw new Error(`Invalid source path: ${source}`);
  }
  const root = sourceRoots.find((root) => source.startsWith(root.ts));
  if (!root || !isSource(source)) throw new Error(`Invalid source path: ${source}`);
  const path = root.manifest + source.slice(root.ts.length) + '.toml';
  if (sourceRoots.find((candidate) => candidate.manifest && path.startsWith(candidate.manifest)) !==
    (root.manifest ? root : undefined)) throw new Error(`Manifest path collision: ${source}`);
  return path;
}

export function sourceForManifestPath(path: string): string {
  const root = sourceRoots.find((root) => root.manifest && path.startsWith(root.manifest)) ?? sourceRoots[0];
  const source = root.ts + path.slice(root.manifest.length, -5);
  if (!path.endsWith('.toml') || manifestPathForSource(source) !== path) throw new Error(`Invalid manifest path: ${path}`);
  return source;
}
export const kinds = ['function', 'method', 'field-fn', 'constructor', 'accessor', 'class', 'const-fn', 'const', 'variable-fn', 'variable', 'type', 'enum'] as const;
export type Kind = (typeof kinds)[number];
export interface SymbolIdentity {
  ts: string;
  kind: Kind;
}
export interface SourceSymbols {
  ts: string;
  symbols: SymbolIdentity[];
}

export function walkFiles(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) return walkFiles(path);
    if (entry.isSymbolicLink()) throw new Error(`Refusing symlink: ${path}`);
    return entry.isFile() ? [path] : [];
  }).sort();
}

export function isSource(path: string): boolean {
  const parts = path.split('/');
  return path.endsWith('.ts') && !path.endsWith('.d.ts') &&
    !path.endsWith('.test.ts') && !parts.includes('__tests__') &&
    !parts.includes('__snapshots__') && parts.at(-1) !== 'perfFixture.ts';
}

function diagnosticText(diagnostics: readonly ts.Diagnostic[]): string {
  return ts.formatDiagnosticsWithColorAndContext(diagnostics, {
    getCanonicalFileName: (name) => name,
    getCurrentDirectory: ts.sys.getCurrentDirectory,
    getNewLine: () => '\n',
  });
}

function callable(checker: ts.TypeChecker, node: ts.Node, initializer?: ts.Expression): boolean {
  if (checker.getNonNullableType(checker.getTypeAtLocation(node)).getCallSignatures().length > 0) return true;
  if (!initializer) return false;
  if (ts.isArrowFunction(initializer) || ts.isFunctionExpression(initializer)) return true;
  if (ts.isParenthesizedExpression(initializer) || ts.isAsExpression(initializer) ||
    ts.isTypeAssertionExpression(initializer) || ts.isSatisfiesExpression(initializer) || ts.isNonNullExpression(initializer)) {
    return callable(checker, initializer.expression, initializer.expression);
  }
  return checker.getNonNullableType(checker.getTypeAtLocation(initializer)).getCallSignatures().length > 0;
}

function memberName(member: ts.NamedDeclaration & { name: ts.PropertyName }, source: ts.SourceFile): string {
  if (ts.isComputedPropertyName(member.name)) return member.name.getText(source);
  return member.name.text;
}

function unwrap(expression: ts.Expression): ts.Expression {
  while (ts.isParenthesizedExpression(expression) || ts.isAsExpression(expression) ||
    ts.isTypeAssertionExpression(expression) || ts.isSatisfiesExpression(expression) || ts.isNonNullExpression(expression)) {
    expression = expression.expression;
  }
  return expression;
}

function reachableDeclarations(exported: Set<ts.Node>, source: ts.SourceFile, checker: ts.TypeChecker): Set<ts.Node> {
  // Module initialization can register callbacks on imported services (e.g. noble hashes).
  // These execute when the module loads even without a reference from an export body.
  const roots = [...exported, ...source.statements.filter(ts.isExpressionStatement)];
  const reachable = new Set<ts.Node>(roots);
  const pending = [...roots];
  for (let index = 0; index < pending.length; index++) {
    const visit = (node: ts.Node) => {
      if (ts.isIdentifier(node)) {
        let symbol = ts.isShorthandPropertyAssignment(node.parent)
          ? checker.getShorthandAssignmentValueSymbol(node.parent) : checker.getSymbolAtLocation(node);
        if (symbol && symbol.flags & ts.SymbolFlags.Alias) symbol = checker.getAliasedSymbol(symbol);
        for (const declaration of symbol?.declarations ?? []) {
          if (declaration.getSourceFile() === source && !reachable.has(declaration)) {
            reachable.add(declaration);
            pending.push(declaration);
          }
        }
      }
      ts.forEachChild(node, visit);
    };
    visit(pending[index]);
  }
  return reachable;
}

function externalField(checker: ts.TypeChecker, node: ts.Node, packageRoot: string): boolean {
  const type = checker.getNonNullableType(checker.getTypeAtLocation(node));
  const declarations = (type.aliasSymbol ?? type.getSymbol())?.declarations ?? [];
  return declarations.some((declaration) => {
    const path = relative(packageRoot, declaration.getSourceFile().fileName);
    return path === '..' || path.startsWith(`..${sep}`);
  });
}

function behaviorInitializer(checker: ts.TypeChecker, expression: ts.Expression, packageRoot: string, seen = new Set<ts.Node>()): boolean {
  const value = unwrap(expression);
  if (seen.has(value)) return false;
  seen.add(value);
  if (ts.isArrowFunction(value) || ts.isFunctionExpression(value)) return true;
  if (ts.isCallExpression(value)) {
    if (ts.isPropertyAccessExpression(value.expression) && value.expression.name.text === 'bind') return true;
    return value.arguments.some((argument) => behaviorInitializer(checker, argument, packageRoot, seen));
  }
  if (ts.isPropertyAccessExpression(value) || ts.isElementAccessExpression(value)) {
    return !!checker.getSymbolAtLocation(value)?.declarations?.some((declaration) =>
      ts.isMethodDeclaration(declaration) || ts.isMethodSignature(declaration));
  }
  if (ts.isIdentifier(value)) {
    let symbol = checker.getSymbolAtLocation(value);
    if (symbol && symbol.flags & ts.SymbolFlags.Alias) symbol = checker.getAliasedSymbol(symbol);
    return !!symbol?.declarations?.some((declaration) => {
      const path = relative(packageRoot, declaration.getSourceFile().fileName);
      if (path === '..' || path.startsWith(`..${sep}`)) return false;
      if (ts.isFunctionDeclaration(declaration)) return !!declaration.body;
      return ts.isVariableDeclaration(declaration) && !!declaration.initializer &&
        behaviorInitializer(checker, declaration.initializer, packageRoot, seen);
    });
  }
  return false;
}

function assignedFields(declaration: ts.ClassDeclaration, checker: ts.TypeChecker): Map<ts.Symbol, ts.Expression[]> {
  const assignments = new Map<ts.Symbol, ts.Expression[]>();
  const visit = (node: ts.Node) => {
    if (ts.isBinaryExpression(node) && node.operatorToken.kind === ts.SyntaxKind.EqualsToken &&
      (ts.isPropertyAccessExpression(node.left) || ts.isElementAccessExpression(node.left)) &&
      node.left.expression.kind === ts.SyntaxKind.ThisKeyword) {
      const symbol = checker.getSymbolAtLocation(node.left);
      if (symbol) assignments.set(symbol, [...assignments.get(symbol) ?? [], node.right]);
    }
    ts.forEachChild(node, visit);
  };
  visit(declaration);
  return assignments;
}

function fieldBehavior(checker: ts.TypeChecker, node: ts.Node, packageRoot: string, initializers: ts.Expression[]): boolean {
  const initializer = ts.isPropertyDeclaration(node) || ts.isParameter(node) || ts.isPropertyAssignment(node)
    ? node.initializer : undefined;
  if (!callable(checker, node, initializer)) return false;
  return !externalField(checker, node, packageRoot) || initializers.some((value) => behaviorInitializer(checker, value, packageRoot));
}

function classMemberPath(className: string, member: ts.ClassElement & { name: ts.PropertyName }, source: ts.SourceFile): string {
  const modifiers = ts.canHaveModifiers(member) ? ts.getModifiers(member) : undefined;
  const staticMember = modifiers?.some((modifier) => modifier.kind === ts.SyntaxKind.StaticKeyword);
  const accessor = ts.isGetAccessorDeclaration(member) ? 'get ' : ts.isSetAccessorDeclaration(member) ? 'set ' : '';
  const privateMember = !staticMember && !accessor && !ts.isPrivateIdentifier(member.name) &&
    modifiers?.some((modifier) => modifier.kind === ts.SyntaxKind.PrivateKeyword);
  return `${className}${privateMember ? '#' : '.'}${staticMember ? 'static ' : ''}${accessor}${memberName(member, source)}`;
}

// These provider shapes need an explicit naming rule before they can enter the corpus.
// Inspect value structure only: callbacks inside a method body are not provider members.
function guardProviderShape(value: ts.Expression, name: string, source: ts.SourceFile, checker: ts.TypeChecker): void {
  const hasCallableProperties = (expression: ts.Expression): boolean => {
    const object = unwrap(expression);
    if (ts.isObjectLiteralExpression(object)) {
      return object.properties.some((property) => {
        if (ts.isSpreadAssignment(property)) return hasCallableProperties(property.expression);
        if (callable(checker, property)) return true;
        return ts.isPropertyAssignment(property) && hasCallableProperties(property.initializer);
      });
    }
    // Require concrete provider-member declarations, excluding library prototype methods.
    const type = checker.getTypeAtLocation(object);
    if (!(type.flags & ts.TypeFlags.Object)) return false;
    return type.getProperties().some((property) =>
      property.declarations?.some((declaration) => ts.isMethodDeclaration(declaration) ||
        ts.isPropertyAssignment(declaration) || ts.isShorthandPropertyAssignment(declaration)) &&
      checker.getNonNullableType(checker.getTypeOfSymbolAtLocation(property, object)).getCallSignatures().length > 0);
  };
  const reject = (shape: string): never => {
    throw new Error(`${source.fileName}: unsupported provider shape for ${name}: ${shape}; define manifest naming before extraction`);
  };
  const object = unwrap(value);
  if (ts.isClassExpression(object)) reject('class expression');
  if (ts.isCallExpression(object) && ts.isPropertyAccessExpression(object.expression) &&
    object.expression.expression.getText(source) === 'Object' && object.expression.name.text === 'freeze' &&
    object.arguments.some(hasCallableProperties)) reject('Object.freeze');
  if (!ts.isObjectLiteralExpression(object)) return;
  for (const property of object.properties) {
    if (ts.isSpreadAssignment(property)) {
      if (hasCallableProperties(property.expression)) reject('callable spread');
    } else if (ts.isPropertyAssignment(property)) {
      const nested = unwrap(property.initializer);
      if (ts.isObjectLiteralExpression(nested) && hasCallableProperties(nested)) reject('nested callable object');
    }
  }
}

export function extractFile(source: ts.SourceFile, checker: ts.TypeChecker, packageRoot: string): SymbolIdentity[] {
  const module = checker.getSymbolAtLocation(source);
  const exported = new Set<ts.Node>();
  if (module) {
    for (let symbol of checker.getExportsOfModule(module)) {
      if (symbol.flags & ts.SymbolFlags.Alias) symbol = checker.getAliasedSymbol(symbol);
      for (const declaration of symbol.declarations ?? []) {
        if (declaration.getSourceFile() === source) exported.add(declaration);
      }
    }
  }
  const reachable = reachableDeclarations(exported, source, checker);
  const symbols: SymbolIdentity[] = [];
  const seen = new Map<string, ts.Symbol | ts.Node>();
  const add = (name: string, kind: Kind, declaration: ts.NamedDeclaration) => {
    const key = `${kind}:${name}`;
    const identity = declaration.name ? checker.getSymbolAtLocation(declaration.name) ?? declaration : declaration;
    // Overloads share a compiler symbol; distinct declarations must not silently share port metadata.
    if (seen.has(key) && seen.get(key) !== identity) throw new Error(`${source.fileName}: ambiguous symbol ${name} (${kind})`);
    if (!seen.has(key)) {
      symbols.push({ ts: name, kind });
      seen.set(key, identity);
    }
  };
  for (const statement of source.statements) {
    if (ts.isVariableStatement(statement)) {
      const immutable = !!(statement.declarationList.flags & ts.NodeFlags.Const);
      for (const declaration of statement.declarationList.declarations) {
        const visitBinding = (name: ts.BindingName, owner: ts.VariableDeclaration | ts.BindingElement) => {
          if (ts.isIdentifier(name)) {
            if (!immutable && !exported.has(owner)) return;
            const initializer = ts.isVariableDeclaration(owner) || ts.isBindingElement(owner) ? owner.initializer : undefined;
            if (reachable.has(owner) && callable(checker, name, initializer)) add(name.text, immutable ? 'const-fn' : 'variable-fn', owner);
            // Exported values carry policy too; inline Rust values still need an explicit mapping.
            if (exported.has(owner) && !callable(checker, name, initializer)) add(name.text, immutable ? 'const' : 'variable', owner);
            if (exported.has(owner) && initializer) guardProviderShape(initializer, name.text, source, checker);
            const object = initializer ? unwrap(initializer) : undefined;
            if (exported.has(owner) && object && ts.isObjectLiteralExpression(object)) {
              for (const property of object.properties) {
                if (ts.isSpreadAssignment(property)) continue;
                const value = ts.isPropertyAssignment(property) ? property.initializer : undefined;
                if (fieldBehavior(checker, property, packageRoot, value ? [value] : [])) {
                  add(`${name.text}.${memberName(property, source)}`, 'field-fn', property);
                }
              }
            }
          } else {
            for (const element of name.elements) {
              if (ts.isBindingElement(element)) visitBinding(element.name, element);
            }
          }
        };
        visitBinding(declaration.name, declaration);
      }
      continue;
    }
    if (!exported.has(statement) && !((ts.isClassDeclaration(statement) || ts.isFunctionDeclaration(statement)) && reachable.has(statement))) continue;
    if (ts.isFunctionDeclaration(statement)) {
      add(statement.name?.text ?? 'default', 'function', statement);
    } else if (ts.isTypeAliasDeclaration(statement) || ts.isInterfaceDeclaration(statement)) {
      add(statement.name.text, 'type', statement);
    } else if (ts.isEnumDeclaration(statement)) {
      add(statement.name.text, 'enum', statement);
    } else if (ts.isClassDeclaration(statement)) {
      const className = statement.name?.text ?? 'default';
      const assignments = assignedFields(statement, checker);
      add(className, 'class', statement);
      for (const member of statement.members) {
        if (ts.isConstructorDeclaration(member)) {
          add(`${className}.constructor`, 'constructor', statement);
          for (const parameter of member.parameters) {
            if (ts.isParameterPropertyDeclaration(parameter, member) && ts.isIdentifier(parameter.name) &&
              fieldBehavior(checker, parameter, packageRoot, parameter.initializer ? [parameter.initializer] : [])) {
              const privateProperty = parameter.modifiers?.some((modifier) => modifier.kind === ts.SyntaxKind.PrivateKeyword);
              add(`${className}${privateProperty ? '#' : '.'}${parameter.name.text}`, 'field-fn', parameter);
            }
          }
          continue;
        }
        if (ts.isGetAccessorDeclaration(member) || ts.isSetAccessorDeclaration(member)) {
          add(classMemberPath(className, member, source), 'accessor', member);
          continue;
        }
        const initializers = ts.isPropertyDeclaration(member) ? member.initializer ? [member.initializer] :
          assignments.get(checker.getSymbolAtLocation(member.name)!) ?? [] : [];
        const kind = ts.isMethodDeclaration(member) ? 'method' :
          ts.isPropertyDeclaration(member) && fieldBehavior(checker, member, packageRoot, initializers) ? 'field-fn' : undefined;
        if (!kind || !(ts.isMethodDeclaration(member) || ts.isPropertyDeclaration(member))) continue;
        add(classMemberPath(className, member, source), kind, member);
      }
    }
  }
  return symbols;
}

export function extractSources(repoRoot: string): SourceSymbols[] {
  const configPath = resolve(repoRoot, 'packages/safe-cex/tsconfig.json');
  const config = ts.readConfigFile(configPath, ts.sys.readFile);
  if (config.error) throw new Error(diagnosticText([config.error]));
  const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, resolve(configPath, '..'), undefined, configPath);
  if (parsed.errors.length) throw new Error(diagnosticText(parsed.errors));
  const scopedFiles = sourceRoots.flatMap((root) => {
    const sourceRoot = resolve(repoRoot, root.ts);
    return existsSync(sourceRoot) ? walkFiles(sourceRoot)
      .filter((path) => isSource(relative(sourceRoot, path).split(sep).join('/')))
      .map((path) => ({ path, packageRoot: resolve(repoRoot, root.package) })) : [];
  }).sort((a, b) => a.path < b.path ? -1 : a.path > b.path ? 1 : 0);
  const files = scopedFiles.map(({ path }) => path);
  const program = ts.createProgram({
    rootNames: [...new Set([...parsed.fileNames, ...files])],
    options: parsed.options,
    projectReferences: parsed.projectReferences,
  });
  const checker = program.getTypeChecker();
  const diagnostics = [...program.getOptionsDiagnostics(), ...program.getGlobalDiagnostics(),
    ...program.getSyntacticDiagnostics(), ...program.getSemanticDiagnostics()];
  if (diagnostics.length) throw new Error(diagnosticText(diagnostics));
  return scopedFiles.map(({ path, packageRoot }) => {
    const source = program.getSourceFile(path);
    if (!source) throw new Error(`Source missing from Program: ${path}`);
    return { ts: relative(repoRoot, path).split(sep).join('/'), symbols: extractFile(source, checker, packageRoot) };
  });
}
