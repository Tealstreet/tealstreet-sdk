// SDK schema-map comparisons over the actual vendored maps and Utils methods.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const assert=require('node:assert/strict'),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..'),dir=path.join(root,'thirdparty/grvt-js-client/src/interfaces/codegen/schema-maps');
const cache=new Map();
function load(file){
 file=path.resolve(file);if(cache.has(file))return cache.get(file);
 const exports={};cache.set(file,exports);const source=fs.readFileSync(file,'utf8');
 const tree=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);assert.equal(tree.parseDiagnostics.length,0);
 const code=ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},reportDiagnostics:true});assert.equal(code.diagnostics.length,0);
 const require=p=>{
  if(p==='../interfaces')return load(path.join(dir,'../hex-string-map.ts'));
  // schemaMap/coverBigInt do not consume these unrelated re-exports.
  if(p==='./json.utils'||p==='./string.utils')return {};
  return load(path.resolve(path.dirname(file),p+'.ts'));
 };
 vm.runInNewContext(code.outputText,{exports,require,Object,Array,Number,BigInt});return exports;
}
const {Utils}=load(path.join(root,'thirdparty/grvt-js-client/src/utils/index.ts'));
const schemas=[
 ['scope','api_positions_request'],['cancel','api_cancel_order_request'],['account','api_get_all_initial_leverage_request'],['set_leverage','api_set_initial_leverage_request'],
 ['create_request','api_create_order_request'],['order_response','api_create_order_response'],['orders_response','api_open_orders_response'],['positions_response','api_positions_response'],
 ['leverage_response','api_get_all_initial_leverage_response'],['success_response','api_set_initial_leverage_response'],['cancel_all_response','api_cancel_all_orders_response'],['funding_response','api_funding_account_summary_response'],['error','error'],
];
function fixture(map,prefix){const out={};for(const[key,raw]of Object.entries(map)){
 const [target,child]=Array.isArray(raw)?raw:[raw];
 out[key]=child?(Array.isArray(child)?[fixture(child[0],prefix+'.'+key+'.0'),fixture(child[0],prefix+'.'+key+'.1')]:fixture(child,prefix+'.'+key)):`${prefix}/${key}/${target}`;
}return out;}
const rows=[];
for(const[schema,file]of schemas){
 const pair=Object.values(load(path.join(dir,file+'.ts')))[0];
 for(const lite of [true,false]){
  const map=lite?pair.FULL_TO_LITE:pair.LITE_TO_FULL;
  const input=fixture(map,schema+(lite?'/full':'/lite'));
  rows.push({schema,lite,input,output:Utils.schemaMap(input,map,lite)});
  const extra={...input,unknownFirst:17,unknownLast:29};
  rows.push({schema,lite,input:extra,output:Utils.schemaMap(extra,map,lite)});
 }
}
const bigintInput={order:{order_id:257n,builder:15n,signature:{signer:1n,r:2n,s:3n,nonce:4n},metadata:{client_order_id:5n}}};
const create=Object.values(load(path.join(dir,'api_create_order_request.ts')))[0];
rows.push({schema:'create_request',lite:true,input:bigintInput,output:Utils.schemaMap(bigintInput,create.FULL_TO_LITE,true)});
assert.equal(rows.length,53);
process.stdout.write(JSON.stringify(rows,(_,v)=>typeof v==='bigint'?{$bigint:v.toString()}:v));
