// Mutate one actual maintained declaration in memory, never the reference file.
const assert = require('node:assert/strict');
module.exports = function controlled(code) {
  if (!process.env.SAFE_CEX_APEX_BOUNDARY_SOURCE_CONTROL) return code;
  const selector = 'const compile=ts.transpileModule(declarations,';
  assert.equal(code.split(selector).length, 2, 'APEX-SOURCE compiler selector');
  return code.replace(selector, () => String.raw`
const compile=ts.transpileModule((()=>{
 const controls={position:['mapPosition','unrealizedPnl'],markets:['fetchMarketsHelper','amount'],
                 ws:['recordWsExecution','price'],history:['fetchHistoricalFills','price'],candles:['fetchOHLCV','timestamp'],update:['updateOrder','prefix'],order:['mapOrder','orderAmount']};
 const control=controls[process.env.SAFE_CEX_APEX_BOUNDARY_SOURCE_CONTROL];
 assert(control,'APEX-SOURCE named control');
 const ast=ts.createSourceFile('boundary-control.ts',declarations,ts.ScriptTarget.Latest,true);
 const cls=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='ApexOmniExchange');
 const member=cls.members.find(n=>n.name?.getText(ast)===control[0]);
 assert(member,'APEX-SOURCE actual member');
 if(control[1]==='prefix'){
  const block=member.initializer.body;
  assert(ts.isBlock(block),'APEX-SOURCE actual async body');
  return declarations.slice(0,block.getStart(ast)+1)+'await Promise.resolve();'+declarations.slice(block.getStart(ast)+1);
 }
 const nodes=[];
 function visit(n){
  if(control[1]==='orderAmount' ? ts.isPropertyAssignment(n)&&n.name.getText(ast)==='amount'&&n.initializer.getText(ast)==='parseFloat(o.size ?? 0)' : (control[1]==='amount'||control[1]==='timestamp')
    ? ts.isPropertyAssignment(n)&&n.name.getText(ast)===control[1]&&n.initializer.getText(ast)===(control[1]==='amount'?'parseFloat(market.stepSize)':'data.t / 1000')
    : ts.isShorthandPropertyAssignment(n)&&n.name.getText(ast)===control[1]) nodes.push(n);
  ts.forEachChild(n,visit);
 }
 visit(member);assert.equal(nodes.length,1,'APEX-SOURCE unique actual output selector');
 const node=nodes[0];
 const replacement=control[1]==='orderAmount'?'amount: parseFloat(o.size ?? 0)+1':control[1]==='amount'?'amount: parseFloat(market.stepSize)+1':control[1]==='timestamp'?'timestamp: data.t / 1000+1':control[1]+': '+control[1]+'+1';
 return declarations.slice(0,node.getStart(ast))+replacement+declarations.slice(node.end);
})(),`);
};
