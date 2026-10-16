// Live source inventory: getters participate in dynamic venue dispatch too.
// Callable properties include branded/wrapped fallbacks.
const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const files = [];
function walk(directory) {
  for (const entry of fs.readdirSync(directory, {withFileTypes:true})) {
    const file = path.join(directory, entry.name);
    if (entry.isDirectory() && entry.name !== '__tests__') walk(file);
    else if (entry.isFile() && file.endsWith('.ts')) files.push(file);
  }
}
walk(path.join(root, 'packages/safe-cex/src/exchanges'));
const program = ts.createProgram(files, {
  target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.ESNext,
  moduleResolution: ts.ModuleResolutionKind.Bundler, skipLibCheck:true,
});
const checker = program.getTypeChecker();
const classes = [];
for (const file of files) {
  const source = program.getSourceFile(file);
  const visit = node => {
    if (ts.isClassDeclaration(node) && node.name) classes.push(node);
    ts.forEachChild(node, visit);
  };
  visit(source);
}
const base = classes.find(node => node.name.text === 'BaseExchange' && node.getSourceFile().fileName === path.join(root, 'packages/safe-cex/src/exchanges/base.ts'));
if (!base) throw new Error('BaseExchange source class missing');
function callableMembers(node) {
  return node.members.filter(member => member.name &&
    !member.modifiers?.some(modifier => modifier.kind === ts.SyntaxKind.StaticKeyword) &&
    (ts.isGetAccessorDeclaration(member) || ts.isMethodDeclaration(member) || checker.getTypeAtLocation(member).getCallSignatures().length));
}
function name(member) {
  if (!ts.isIdentifier(member.name) && !ts.isStringLiteral(member.name)) throw new Error(`Unsupported computed exchange member ${member.name.getText()}`);
  return member.name.text;
}
const baseNames = new Set(callableMembers(base).map(name));
function descendant(node, seen = new Set()) {
  if (node === base) return true;
  if (seen.has(node)) throw new Error(`Exchange inheritance cycle: ${node.name.text}`);
  seen.add(node);
  const parent = node.heritageClauses?.find(clause => clause.token === ts.SyntaxKind.ExtendsKeyword)?.types[0]?.expression;
  if (!parent) return false;
  let symbol = checker.getSymbolAtLocation(parent);
  if (symbol?.flags & ts.SymbolFlags.Alias) symbol = checker.getAliasedSymbol(symbol);
  const declaration = symbol?.declarations?.find(ts.isClassDeclaration);
  if (!declaration && parent.getText() === 'Error') return false;
  if (!declaration) throw new Error(`Unresolved class ancestor ${node.name.text} -> ${parent.getText()}`);
  return descendant(declaration, seen);
}
const rows = [];
for (const node of classes) {
  if (node === base || !descendant(node)) continue;
  for (const member of callableMembers(node)) {
    const method = name(member);
    if (baseNames.has(method)) rows.push({
      method, class:node.name.text,
      file:path.relative(root,node.getSourceFile().fileName).split(path.sep).join('/'),
    });
  }
}
rows.sort((a,b) => a.file < b.file ? -1 : a.file > b.file ? 1 : a.class < b.class ? -1 : a.class > b.class ? 1 : a.method < b.method ? -1 : a.method > b.method ? 1 : 0);
process.stdout.write(JSON.stringify(rows, null, 2) + "\n");
