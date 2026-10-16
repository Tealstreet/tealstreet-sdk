// Syntax inventory for the notification walk; injected callbacks are declarations,
// while their caller-supplied implementations lie outside this source inventory.
export function collectCallableDeclarations(ts, file, source, addDeclaration) {
  const aliases = new Map();
  for (const statement of source.statements) {
    if (ts.isTypeAliasDeclaration(statement)) aliases.set(statement.name.text, statement.type);
  }
  function callableType(type, visiting = new Set()) {
    if (!type) return false;
    if (ts.isFunctionTypeNode(type)) return true;
    if (ts.isParenthesizedTypeNode(type)) return callableType(type.type, visiting);
    if (ts.isUnionTypeNode(type)) return type.types.some((part) => callableType(part, visiting));
    if (ts.isTypeReferenceNode(type) && ts.isIdentifier(type.typeName)) {
      const name = type.typeName.text;
      if (visiting.has(name)) return false;
      return callableType(aliases.get(name), new Set([...visiting, name]));
    }
    return false;
  }
  function visit(node) {
    if (ts.isClassDeclaration(node) && node.name) {
      const owner = node.name.text;
      for (const member of node.members) {
        const name = member.name?.getText(source).replace(/^#/, '') ?? '';
        const key = `${owner}.${name}`;
        if (member.body) addDeclaration(file, key, member, source, 'body');
        else if (ts.isPropertyDeclaration(member)) {
          if (member.initializer && (ts.isArrowFunction(member.initializer) || ts.isFunctionExpression(member.initializer))) {
            addDeclaration(file, key, member.initializer, source, 'body');
          } else if (callableType(member.type)) {
            addDeclaration(file, key, member, source, 'injected-function-property');
          }
        }
        if (ts.isConstructorDeclaration(member)) {
          for (const parameter of member.parameters) {
            if (ts.isParameterPropertyDeclaration(parameter, member) && callableType(parameter.type)) {
              addDeclaration(file, `${owner}.${parameter.name.getText(source)}`, parameter, source, 'injected-function-property');
            }
          }
        }
      }
    } else if (ts.isFunctionDeclaration(node) && node.body) {
      addDeclaration(file, node.name?.text ?? '', node, source, 'body');
    } else if (ts.isVariableDeclaration(node) && node.initializer
      && (ts.isArrowFunction(node.initializer) || ts.isFunctionExpression(node.initializer))) {
      addDeclaration(file, node.name.getText(source), node.initializer, source, 'body');
    }
    ts.forEachChild(node, visit);
  }
  visit(source);
}
