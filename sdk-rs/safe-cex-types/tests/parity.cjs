const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const printer = ts.createPrinter();

function dependencies(source, roots, boundaries = []) {
  const index = new Map(source.statements.filter(s => ts.isTypeAliasDeclaration(s) || ts.isEnumDeclaration(s) || ts.isInterfaceDeclaration(s))
    .map(s => [s.name.text, s]));
  const needed = new Set();
  const follow = name => {
    if (needed.has(name) || boundaries.includes(name)) return;
    const declaration = index.get(name);
    assert(declaration, `${name}: missing oracle declaration`);
    assert(!ts.isInterfaceDeclaration(declaration) || !declaration.heritageClauses?.length, `${name}: unexpected interface heritage`);
    needed.add(name);
    const visit = node => {
      if (ts.isTypeReferenceNode(node)) {
        assert(ts.isIdentifier(node.typeName), 'qualified type references need explicit parity support');
        if (!["Array", "Record", "Partial", "Pick", "Omit", "Promise"].includes(node.typeName.text)) follow(node.typeName.text);
      }
      ts.forEachChild(node, visit);
    };
    visit(declaration);
  };
  roots.forEach(follow);
  const declarations = source.statements.filter(s => needed.has(s.name?.text));
  return { declarations, enums: declarations.filter(ts.isEnumDeclaration).map(s => s.name.text) };
}

function normalized(node) {
  if (ts.isLiteralTypeNode(node)) return ['literal', node.literal.kind, node.literal.text ?? node.literal.kind];
  if (ts.isTypeReferenceNode(node) && node.typeName.getText() !== 'Array') return ['ref', node.typeName.getText(), ...(node.typeArguments || []).map(normalized)];
  if (ts.isArrayTypeNode(node)) return ['array', normalized(node.elementType)];
  if (ts.isTypeReferenceNode(node) && node.typeName.getText() === 'Array') return ['array', normalized(node.typeArguments[0])];
  if (ts.isTypeLiteralNode(node)) return ['object', ...node.members.map(member => [
    ts.isIndexSignatureDeclaration(member) ? 'index' : member.name.text,
    Boolean(member.questionToken), Boolean(member.modifiers?.some(m => m.kind === ts.SyntaxKind.ReadonlyKeyword)),
    ts.isIndexSignatureDeclaration(member) ? member.parameters.map(p => normalized(p.type)) : [],
    normalized(member.type),
  ])];
  if (ts.isUnionTypeNode(node)) return ['union', ...node.types.map(normalized)];
  return printer.printNode(ts.EmitHint.Unspecified, node, node.getSourceFile());
}

function nestedFields(node) {
  let count = ts.isPropertySignature(node) || ts.isIndexSignatureDeclaration(node) ? 1 : 0;
  ts.forEachChild(node, child => { count += nestedFields(child); });
  return count;
}

function checkFields(expected, actual, prefix) {
  assert(ts.isTypeLiteralNode(actual), prefix);
  assert.equal(actual.members.length, expected.members.length, `${prefix}: field count`);
  let count = 0;
  for (let i = 0; i < expected.members.length; i++) {
    const a = expected.members[i], b = actual.members[i];
    const key = ts.isIndexSignatureDeclaration(a) ? '[key]' : a.name.text;
    const name = `${prefix}.${key}`;
    assert.equal(ts.isIndexSignatureDeclaration(b) ? '[key]' : b.name.text, key, `${name}: name/order`);
    assert.equal(Boolean(b.questionToken), Boolean(a.questionToken), `${name}: optionality`);
    const readonly = n => Boolean(n.modifiers?.some(m => m.kind === ts.SyntaxKind.ReadonlyKeyword));
    assert.equal(readonly(b), readonly(a), `${name}: readonly`);
    if (ts.isIndexSignatureDeclaration(a)) {
      assert(ts.isIndexSignatureDeclaration(b), `${name}: index signature`);
      assert.deepEqual(b.parameters.map(p => normalized(p.type)), a.parameters.map(p => normalized(p.type)), `${name}: key type`);
    }
    count++;
    if (ts.isTypeLiteralNode(a.type)) count += checkFields(a.type, b.type, name);
    else {
      assert.deepEqual(normalized(b.type), normalized(a.type), `${name}: type`);
      count += nestedFields(a.type);
    }
  }
  return count;
}

function shape(declaration, index) {
  assert(!ts.isInterfaceDeclaration(declaration) || !declaration.heritageClauses?.length, `${declaration.name.text}: unexpected interface heritage`);
  const resolve = node => {
    if (ts.isTypeLiteralNode(node)) return node;
    if (ts.isIntersectionTypeNode(node)) {
      const members = node.types.flatMap(t => resolve(t).members);
      return ts.factory.createTypeLiteralNode(members.filter((m, i) => !members.slice(i + 1).some(n => n.name.text === m.name.text)));
    }
    if (ts.isTypeReferenceNode(node) && node.typeName.text === 'Omit') {
      const members = resolve(index.get(node.typeArguments[0].typeName.text).type).members;
      const excluded = ts.isUnionTypeNode(node.typeArguments[1]) ? node.typeArguments[1].types : [node.typeArguments[1]];
      return ts.factory.createTypeLiteralNode(members.filter(m => !excluded.some(n => n.literal.text === m.name.text)));
    }
    throw new Error('unsupported object shape: ' + node.getText());
  };
  return ts.isInterfaceDeclaration(declaration) ? ts.factory.createTypeLiteralNode(declaration.members) : resolve(declaration.type);
}
module.exports = { dependencies, checkFields, normalized, shape };
