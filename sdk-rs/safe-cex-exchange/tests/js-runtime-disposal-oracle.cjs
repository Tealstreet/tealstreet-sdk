// Source dispose overrides, with observable socket callbacks and abrupt completion.
const fs=require('node:fs'), path=require('node:path'), vm=require('node:vm');
const assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root=path.resolve(__dirname,'../../..');
const rows=[];
for(const [venue,klass] of [['grvt','GrvtExchange'],['cryptocom','CryptocomExchange']]) {
  const file=path.join(root,`packages/safe-cex/src/exchanges/${venue}/${venue}.exchange.ts`);
  const source=fs.readFileSync(file,'utf8');
  const tree=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);
  assert.equal(tree.parseDiagnostics.length,0);
  const cls=tree.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text===klass);
  const member=cls.members.find(n=>n.name?.getText(tree)==='dispose');
  assert.ok(member&&ts.isArrowFunction(member.initializer));
  const text=ts.transpileModule(`const dispose = ${member.initializer.getText(tree)};`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
  for(const fail of ['', 'public', 'private']) {
    const events=[];
    const receiver={isDisposed:false};
    for(const side of ['public','private']) receiver[side+'Websocket']={dispose(){events.push([side,receiver.isDisposed]);if(fail===side) throw side+'-cleanup';}};
    const context={receiver};vm.createContext(context);
    vm.runInContext(`function bind(){${text};return dispose;} globalThis.invoke=bind.call(receiver);`,context);
    let error=null;try{context.invoke()}catch(e){error=e;}
    rows.push({venue,fail,events,disposed:receiver.isDisposed,error});
  }
}
assert.equal(rows.length,6);
process.stdout.write(JSON.stringify(rows));
