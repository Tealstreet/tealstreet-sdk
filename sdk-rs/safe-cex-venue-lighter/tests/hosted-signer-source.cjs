const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
function tree(relative) {
  const file = path.join(root, 'packages/safe-cex/src', relative);
  const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
  assert.equal(ast.parseDiagnostics.length, 0);
  return ast;
}
function owner(ast, name) {
  const cls = ast.statements.find(node => ts.isClassDeclaration(node) && node.name?.text === name);
  assert.ok(cls, name);
  return cls;
}
function probe(ast, cls, name) {
  const member = cls.members.find(node => node.name?.getText(ast) === name);
  assert.ok(member, name);
  let text = member.getText(ast);
  if (name === 'ensureSignerAdapter' && process.argv.includes('--drop-secret-guard')) {
    const guards = member.initializer.body.statements.filter(node => {
      if (!ts.isIfStatement(node) || !ts.isPrefixUnaryExpression(node.expression)) return false;
      const expression = node.expression;
      return expression.operator === ts.SyntaxKind.ExclamationToken &&
        ts.isPropertyAccessExpression(expression.operand) && expression.operand.name.text === 'secret' &&
        expression.operand.expression.getText(ast) === 'this.options';
    });
    assert.equal(guards.length, 1, 'one source local-key guard');
    const start = member.getStart(ast);
    text = text.slice(0, guards[0].getStart(ast) - start) + text.slice(guards[0].end - start);
  }
  const compiled = ts.transpileModule('class Probe { ' + text + ' } globalThis.Probe = Probe;', {
    compilerOptions: { target: ts.ScriptTarget.ES2022 }, reportDiagnostics: true,
  });
  assert.equal(compiled.diagnostics.length, 0);
  const context = vm.createContext({});
  vm.runInContext(compiled.outputText, context);
  return new context.Probe();
}
(async () => {
  const exchange = tree('exchanges/lighter/lighter.exchange.ts');
  const exchangeClass = owner(exchange, 'LighterExchange');
  let xhrUses = 0;
  const walk = node => {
    if (ts.isPropertyAccessExpression(node) && node.expression.kind === ts.SyntaxKind.ThisKeyword && node.name.text === 'xhr') xhrUses++;
    ts.forEachChild(node, walk);
  };
  walk(exchangeClass);
  assert.equal(xhrUses, 0, 'Lighter source never installs or uses BaseExchange.xhr');
  const signer = tree('exchanges/lighter/lighter.signer.ts');
  const base = tree('exchanges/base.ts');
  const rows = [];
  for (const name of ['lighter', 'lighter-robinhood']) {
    let remoteCalls = 0;
    const remoteSigner = { sign: () => { remoteCalls++; throw new Error('unexpected remote call'); } };
    const options = { key: 'hosted-key', secret: '', remoteSigner, apiKeyIndex: '3', applicationId: '42' };
    for (const [label, ast, cls, method, message] of [
      ['private-order-gate', exchange, exchangeClass, 'ensureSignerAdapter', 'Lighter private key is missing'],
      ['sdk-signer-gate', signer, owner(signer, 'TealstreetLighterSignerAdapter'), 'initialize', 'Missing secret for signer initialization'],
    ]) {
      const value = probe(ast, cls, method);
      Object.assign(value, { name, options });
      let error;
      try { await value[method](); } catch (e) { error = e; }
      assert.equal(error?.name, 'Error', label);
      assert.equal(error?.message, message, label);
      assert.equal(remoteCalls, 0, label + ' never substitutes the HTTP signer');
      rows.push({ name, label, error: message, remoteCalls });
    }
    const value = probe(base, owner(base, 'BaseExchange'), 'signHttpRequest');
    Object.assign(value, { name, options });
    const reply = await value.signHttpRequest({ id: name, config: {} });
    assert.equal(reply.id, name);
    assert.equal(reply.error, 'Exchange does not have xhr for signing');
    assert.equal(remoteCalls, 0);
    rows.push({ name, label: 'http-signing', error: reply.error, remoteCalls });
  }
  assert.equal(rows.length, 6);
  process.stdout.write(JSON.stringify(rows));
})().catch(error => { console.error(error); process.exitCode = 1; });
