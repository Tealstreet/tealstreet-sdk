import ts from 'typescript';

// Run in tests, not in measured replays. Parse syntax so comments and string
// literals cannot hide an output dependency or cause false positives.
export function fixerLint(source, filename = 'gate.mjs', entireFile = false) {
  const file = ts.createSourceFile(filename, source, ts.ScriptTarget.Latest, true, ts.ScriptKind.JS);
  const errors = [];
  const inspect = (node) => {
    if ((ts.isIdentifier(node) && node.text === 'rust') ||
        (ts.isElementAccessExpression(node) && ts.isStringLiteral(node.argumentExpression) && node.argumentExpression.text === 'rust'))
      errors.push(`${filename}: fixer reads or declares Rust output at ${file.getLineAndCharacterOfPosition(node.getStart()).line + 1}`);
    ts.forEachChild(node, inspect);
  };
  const inspectFixer = (node) => {
    if (ts.isArrowFunction(node) || ts.isFunctionExpression(node) || ts.isMethodDeclaration(node) || ts.isFunctionDeclaration(node)) {
      if (node.parameters.length > 1 && !['records', 'tapeRecords'].includes(node.parameters[1].name.getText(file)))
        errors.push(`${filename}: fixer accepts candidate output instead of tape records`);
      return;
    }
    ts.forEachChild(node, inspectFixer);
  };
  const visit = (node) => {
    if (ts.isVariableDeclaration(node) && node.name.getText(file) === 'KNOWN_TS_FIXES') {
      inspect(node.initializer);
      inspectFixer(node.initializer);
      return;
    }
    if ((ts.isVariableDeclaration(node) || ts.isFunctionDeclaration(node)) && /^fix[A-Z]|^normalizeBingx/.test(node.name?.getText(file) ?? '')) {
      inspect(node);
      inspectFixer(ts.isVariableDeclaration(node) ? node.initializer : node);
      return;
    }
    ts.forEachChild(node, visit);
  };
  if (entireFile) inspect(file);
  visit(file);
  return errors;
}
