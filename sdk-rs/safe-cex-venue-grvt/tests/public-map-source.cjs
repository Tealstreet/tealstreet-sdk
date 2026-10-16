'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/grvt/grvt.ws-public.ts');
const tree = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const declaration = tree.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'GrvtPublicWebSocket');
const member = declaration.members.find(n => n.name?.getText(tree) === 'listenOrderBook');
assert(member, 'GRVT MAP actual listener member');
const output = ts.transpileModule(`class Base {}\nexport class Venue extends Base { ${member.getText(tree)} }`, {
  compilerOptions: {module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}, reportDiagnostics:true,
});
assert.equal(output.diagnostics.length, 0);
const context = {exports:{}, setTimeout(){ throw Error('unexpected timer'); }};
vm.runInNewContext(output.outputText, context, {filename:file});
const names = ['ordinary','empty','throw','noncallable','getter-throw','retained','arbitrary','accessor-output','replace-asks','result-alias'];
let completed = 0;
for (const name of names) {
  const events = [], books = [], cause = Error('map-fail');
  const bids = [{p:'12',s:'2'}], asks = [];
  let projector, outputReads = 0;
  const alias = [{price:12,amount:2,total:2}];
  const venue = new context.exports.Venue();
  Object.assign(venue, {isConnected:true, messageHandlers:{}, topics:{},
    parent:{store:{markets:[{id:'BTC',symbol:'BTC/USDT'}]},emitAndLogError(){throw Error('unexpected market error');}},
    subscribeToOrderbook(){return 17;},unsubscribeFromOrderbook(){},cleanupMessageHandlerStats(){},
  });
  Object.defineProperty(asks, 'map', {configurable:true, get(){
    events.push('get-asks');
    return function(){assert.equal(this, asks, 'GRVT MAP source asks receiver');events.push('call-asks');return [];};
  }});
  Object.defineProperty(bids, 'map', {get(){
    events.push('get-bids');
    if(name === 'getter-throw')throw cause;
    if(name === 'noncallable')return 0;
    return function(callback){
      assert.equal(this, bids, 'GRVT MAP source bids receiver');
      events.push('call-bids');
      if(name === 'throw')throw cause;
      if(name === 'retained'){projector = callback;return [];}
      if(name === 'arbitrary')return 42;
      if(name === 'result-alias')return alias;
      if(name === 'accessor-output')return [{get price(){outputReads++;return 12;},amount:2,total:2}];
      if(name === 'replace-asks')Object.defineProperty(asks,'map',{get(){events.push('get-asks-changed');return function(){events.push('call-asks-changed');return [];};}});
      return name === 'ordinary' ? Array.prototype.map.call(this, callback) : [];
    };
  }});
  venue.listenOrderBook('BTC/USDT', book => {events.push('callback');books.push(book);});
  let error;
  try {venue.messageHandlers['orderbook.BTC'][0]({i:'BTC',b:bids,a:asks});}catch(e){error=e;}
  if(['throw','getter-throw'].includes(name))assert.equal(error,cause,'GRVT MAP source original throw');
  else if(name === 'noncallable')assert.equal(error?.name,'TypeError','GRVT MAP source noncallability');
  else assert.equal(error,undefined,'GRVT MAP source successful dispatch');
  const expected = name === 'getter-throw' || name === 'noncallable' ? ['get-bids'] : name === 'throw' ? ['get-bids','call-bids'] : name === 'replace-asks' ? ['get-bids','call-bids','get-asks-changed','call-asks-changed','callback'] : ['get-bids','call-bids','get-asks','call-asks','callback'];
  assert.deepEqual(events,expected,'GRVT MAP source callee and callback order');
  if(!error){
    assert.equal(books.length,1,'GRVT MAP source callback count');
    if(name === 'arbitrary')assert.equal(books[0].bids,42,'GRVT MAP source arbitrary map return');
    else if(name === 'accessor-output')assert.equal(outputReads,0,'GRVT MAP source output getter remains uncalled');
    else assert.equal(books[0].bids.length,name === 'ordinary' || name === 'result-alias' ? 1 : 0,'GRVT MAP source selected levels');
  }
  if(name === 'result-alias'){
    assert.equal(books[0].bids,alias,'GRVT MAP source live callback alias');
    alias[0].total = 9;
    assert.equal(books[0].bids[0].total,9,'GRVT MAP source retained result mutation');
  }
  if(projector){
    let comparisons = 0;
    const level = projector({p:'5',s:'2'},{valueOf(){comparisons++;return 1;}},[{s:'7'}]);
    assert.equal(level.price,5,'GRVT MAP source retained price');
    assert.equal(level.amount,2,'GRVT MAP source retained amount');
    assert.equal(level.total,9,'GRVT MAP source alternate array');
    assert.equal(comparisons,2,'GRVT MAP source repeated comparison');
  }
  completed++;
}
assert.equal(completed,10,'GRVT MAP source completed case count');
process.stdout.write('GRVT MAP completed ten actual-source cases\n');
