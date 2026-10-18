const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
const results = {};
for (const kind of ['public', 'private']) {
  const file = path.resolve(__dirname, `../../../packages/safe-cex/src/exchanges/kucoin/kucoin.ws-${kind}.ts`);
  const source = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
  const cls = source.statements.find(ts.isClassDeclaration);
  const member = cls.members.find(m => m.name?.getText(source) === 'reconnect');
  const body = member.initializer.body.getText(source).slice(1, -1);
  const receiver = {
    isDisposed: false, token: 'old', socketEndpoint: 'old', subscribed: true,
    connectAndSubscribe: () => new Promise(() => {}),
  };
  new AsyncFunction(body).call(receiver);
  results[kind] = {token: receiver.token, endpoint: receiver.socketEndpoint, subscribed: receiver.subscribed};
}
process.stdout.write(JSON.stringify(results));
