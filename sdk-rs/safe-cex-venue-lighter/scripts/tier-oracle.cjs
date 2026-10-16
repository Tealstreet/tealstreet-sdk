const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const source = fs.readFileSync(path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/lighter/lighter.account-tier.ts'), 'utf8');
const out = {};
vm.runInNewContext(ts.transpileModule(source + '\nexport { asRecord };', { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText, { exports: out });
const ast = ts.createSourceFile('tier.ts', source, ts.ScriptTarget.Latest, true);
const tiers = ast.statements.find(n => ts.isTypeAliasDeclaration(n) && n.name.text === 'LighterAccountTier').type.types.map(n => n.literal.text);
const constants = Object.fromEntries(Object.entries(out).filter(([, v]) => typeof v !== 'function'));
function result(run) { try { return { ok: run() ?? null }; } catch (e) { return { error: {name:e.name,message:e.message} }; } }
const inputs = [undefined, null, '', 'standard', 'std', 'premium', 'plus', 'PLUS', '\uFEFFstd\uFEFF', '\u0085plus', ' ＰＬＵＳ ', 0, true, [], ['premium'], [['plus']], ['premium', 'plus'], {}, { toString: null }, {toString:0}, [{toString:false}], [null,'plus'], [''], [null], {valueOf:null}];
const identityRecords=require('./collection-identity.cjs').records;
inputs.push(...identityRecords);
const normalized = inputs.map(value => ({ input: value, result: result(() => out.normalizeLighterAccountTier(value)), record: out.asRecord(value) ?? null }));
const eligible = [undefined, ...tiers].map(tier => ({ tier, expected: out.isLighterAccountTierEligibleForPartnerAttribution(tier) }));
const keys = ['user_tier_name','userTierName','user_tier','userTier','account_type_name','accountTypeName','account_type','accountType'];
const payloadInputs = [...inputs];
for (const key of keys) for (const tier of inputs) for (const location of ['root', 'data', 'account', 'dataAccount']) {
  const record = { [key]: tier };
  payloadInputs.push(location === 'root' ? record : location === 'data' ? {data:record} : location === 'account' ? {account:record} : {data:{account:record}});
}
for (const a of keys) for (const b of keys) if (a !== b) payloadInputs.push({[a]:'premium',[b]:'plus'});
for (const value of [undefined,null,'','bad',0,false,[],{}]) payloadInputs.push({user_tier_name:value,data:{user_tier:'plus'},account:{user_tier:'premium'}});
payloadInputs.push({account:[],data:{account:{user_tier:'plus'}}}, {account:{},data:{account:{user_tier:'plus'}}}, {account:null,data:{account:{user_tier:'plus'}}});
const payloads = payloadInputs.map(input => ({ input, result: result(() => out.extractLighterAccountTierFromPayload(input)) }));
const errors = [];
for (const code of [undefined,null,21520,'\uFEFF21520\uFEFF','21520x',21149,'21149', ['21520'], {toString:null}])
  for (const message of [undefined,null,'Standard integrator requires Plus','standard partner attribution needs PREMIUM','standard integrator account type','premium integrator','INTEGRATOR IS NOT APPROVED','integrator not approved','not approved', ['integrator not approved'], {toString:null}])
    errors.push({code,message,tier:result(() => out.isLighterPartnerAttributionTierRejection({code,message})),approval:result(() => out.isLighterPartnerAttributionApprovalMissingRejection({code,message}))});
const fees = [];
for (const integratorMakerFee of [undefined,0,-0,1,-1,NaN,Infinity,-Infinity,250]) for (const integratorTakerFee of [undefined,0,-0,1,-1,NaN,Infinity,-Infinity,250]) {
  const params = {integratorMakerFee,integratorTakerFee};
  fees.push({...params,expected:out.hasNonZeroLighterIntegratorFee(params)});
}
process.stdout.write(JSON.stringify({tiers,constants,normalized,eligible,payloads,errors,fees}, (_k,v) => typeof v === 'number' && (!Number.isFinite(v)||Object.is(v,-0)) ? {$number:Object.is(v,-0)?'-0':String(v)} : v));
