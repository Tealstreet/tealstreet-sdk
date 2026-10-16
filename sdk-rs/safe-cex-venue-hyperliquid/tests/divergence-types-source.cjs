const fs=require('node:fs'), assert=require('node:assert/strict'), cp=require('node:child_process');
const path=require('node:path');
const root=path.resolve(__dirname,'../../..'), ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const head=cp.execFileSync('git',['rev-parse','HEAD'],{cwd:root,encoding:'utf8'}).trim();
const rows=[{"ts": "OidOrCloid"}, {"ts": "Meta"}, {"ts": "ClearinghouseState"}, {"ts": "UserFills"}, {"ts": "OrderResponseStatus"}, {"ts": "OrderResponse"}, {"ts": "WsTrade"}, {"ts": "WsBook"}, {"ts": "WsBbo"}, {"ts": "WsLevel"}, {"ts": "WsOrder"}, {"ts": "WsFill"}, {"ts": "WsUserFunding"}, {"ts": "WsLiquidation"}, {"ts": "WsNonUserCancel"}, {"ts": "SpotClearinghouseBalance"}, {"ts": "SpotClearinghouseState"}, {"ts": "FrontendOpenOrders"}, {"ts": "L2Book"}, {"ts": "CandleSnapshot"}, {"ts": "AssetCtx"}, {"ts": "MetaAndAssetCtxs"}, {"ts": "UserFundingEntry"}, {"ts": "UserFunding"}, {"ts": "UserNonFundingLedgerEntry"}, {"ts": "UserNonFundingLedgerUpdates"}, {"ts": "FundingHistoryEntry"}, {"ts": "FundingHistory"}, {"ts": "SpotToken"}, {"ts": "SpotMarket"}, {"ts": "SpotMeta"}, {"ts": "SpotAssetCtx"}, {"ts": "SpotMetaAndAssetCtxs"}, {"ts": "UserOpenOrder"}, {"ts": "UserOpenOrders"}, {"ts": "OrderRequest"}, {"ts": "OrderWire"}, {"ts": "CancelOrderRequest"}, {"ts": "ModifyRequest"}, {"ts": "ModifyWire"}, {"ts": "Signature"}, {"ts": "Candle"}, {"ts": "HyperliquidCandle"}, {"ts": "WsUserFill"}, {"ts": "WsUserFills"}, {"ts": "WsUserFundings"}, {"ts": "WsUserNonFundingLedgerUpdate"}, {"ts": "WsUserNonFundingLedgerUpdates"}, {"ts": "Hip3MarketInfo"}, {"ts": "BuilderInfo"}, {"ts": "BuilderFeeCheckResult"}];
assert.equal(rows.length,51,'independent public scalar-string root floor');
const corpus=['typed-string-boundaries.json','typed-null-string-boundaries.json'].flatMap(name=>JSON.parse(fs.readFileSync(root+'/sdk-rs/safe-cex-venue-hyperliquid/tests/fixtures/'+name,'utf8')).cases);
const selected=rows;assert.equal(selected.length,51);
const declarations=[],observations=[],duplicates=[];
function duplicateText(v,steps){if(Array.isArray(v))return "["+v.map((x,i)=>String(i)===steps[0]?duplicateText(x,steps.slice(1)):JSON.stringify(x)).join(",")+"]";return "{"+Object.entries(v).flatMap(([k,x])=>k!==steps[0]?[JSON.stringify(k)+":"+JSON.stringify(x)]:steps.length===1?[JSON.stringify(k)+":\"\\ud800\"",JSON.stringify(k)+":"+JSON.stringify(x)]:[JSON.stringify(k)+":"+duplicateText(x,steps.slice(1))]).join(",")+"}";}
let assignmentCount=0,extensionCount=0;
function at(value,path){for(const part of path.slice(1).split('/').filter(Boolean))value=value[part.replaceAll('~1','/').replaceAll('~0','~')];return value;}
for(const row of selected){
 const cases=corpus.filter(c=>c.name===row.ts);assert.equal(cases.length,3,row.ts+' source input count');
 for(const input of cases){const value=JSON.parse(input.raw),field=at(value,input.path);assert.equal(typeof field,'string',row.ts+' materialized source field');if(input.refusal){assert.equal(field.length,1);assert.ok([0xd800,0xdfff].includes(field.charCodeAt(0)));}else assert.deepEqual(Array.from(field,x=>x.codePointAt(0)),[0x96ea,0x1f600,0xfeff]);
  declarations.push(`const case${assignmentCount++}: Source.${row.ts} = ${input.raw};`);
 }
 const base=cases.find(c=>!c.refusal);const value=JSON.parse(base.raw);
 observations.push({ts:row.ts,path:base.path,negativeUnits:[0xd800,0xdfff],nativeExecuted:false});
 if(row.ts==='OidOrCloid')continue;
 let object=value,expression='base'+extensionCount;
 if(Array.isArray(value)){assert.ok(value.length>0);object=value[0];expression+='[0]';}
 assert.equal(typeof object,'object');assert.ok(object&&!Array.isArray(object));
 const duplicate=duplicateText(JSON.parse(base.raw),base.path.slice(1).split('/'));assert.deepEqual(JSON.parse(duplicate),JSON.parse(base.raw),row.ts+' overwritten unit has no surviving effect');duplicates.push({ts:row.ts,path:base.path,raw:duplicate,sourceValue:at(JSON.parse(duplicate),base.path),nativeExecuted:false});
 let deep={};for(let i=0;i<140;i++)deep={x:deep};object.auditUnknown=deep;
 assert.equal(at(value,base.path),at(JSON.parse(base.raw),base.path),row.ts+' selected value survives unknown extension');
 declarations.push(`const base${extensionCount}: Source.${row.ts} = ${base.raw};`);
 declarations.push(`Object.assign(${expression}, {auditUnknown: deep});`);
 declarations.push(`const extended${extensionCount}: Source.${row.ts} = base${extensionCount};`);
 let cursor=object.auditUnknown;for(let i=0;i<140;i++)cursor=cursor.x;assert.deepEqual(cursor,{});
 extensionCount++;
}
assert.equal(assignmentCount,153);assert.equal(extensionCount,50);
for(const rootName of ['UserRateLimit','OrderStatus']) {
 for(const [shape,raw] of [['object','{"bad":"\\ud800","literal":{"$serde_json::private::Number":"120"}}'],['array','["\\udfff"]']]) {
  const value=JSON.parse(raw),field=shape==='object'?value.bad:value[0];
  assert.equal(field.length,1);assert.equal(field.charCodeAt(0),shape==='object'?0xd800:0xdfff);
  declarations.push(`const ${rootName}${shape}: Source.${rootName} = ${raw};`);
 }
}
const filename=root+'/sdk-rs/safe-cex-venue-hyperliquid/tests/source-review-virtual.ts';
const text='import type * as Source from "../../../packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.types";\nlet deep: unknown = {};\n'+declarations.join('\n');
const options={strict:true,noEmit:true,skipLibCheck:true,types:[],target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS};
const host=ts.createCompilerHost(options),read=host.readFile.bind(host),exists=host.fileExists.bind(host);
host.readFile=p=>p===filename?text:read(p);host.fileExists=p=>p===filename||exists(p);
const program=ts.createProgram([filename],options,host),errors=ts.getPreEmitDiagnostics(program);
assert.equal(errors.length,0,ts.formatDiagnosticsWithColorAndContext(errors,{getCanonicalFileName:x=>x,getCurrentDirectory:()=>root,getNewLine:()=> '\n'}));
assert.equal(duplicates.length,50);const receipt={head,selectedRoots:51,strictAssignments:153,unknownExtensions:50,duplicateOverwriteCases:50,observations,duplicates,nativeExecuted:false};console.log(JSON.stringify({head,selectedRoots:51,strictAssignments:153,unknownExtensions:50,duplicateOverwriteCases:50,nativeExecuted:false}));
