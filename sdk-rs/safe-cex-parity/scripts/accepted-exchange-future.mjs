// Three reviewed production aliases; expected compiler types are authored here.
import assert from 'node:assert/strict';
const path = name => ({ resolved_path: { path: name, args: null } });
const applied = (name, types) => ({ resolved_path: { path: name, args: { angle_bracketed: { args: types.map(type => ({ type })), constraints: [] } } } });
const result = (ok, error) => applied('core::result::Result', [ok, path(error)]);
const future = output => applied('core::pin::Pin', [applied('alloc::boxed::Box', [{ dyn_trait: { traits: [{ trait: { path: 'core::future::future::Future', args: { angle_bracketed: { args: [], constraints: [{ name: 'Output', args: null, binding: { equality: { type: output } } }] } } }, generic_params: [] }], lifetime: "'a" } }])]);
const lifetime = { name: "'a", kind: { lifetime: { outlives: [] } } };
const unbounded = { name: 'T', kind: { type: { bounds: [], default: null, is_synthetic: false } } };
const pinBox = output => `::std::pin::Pin<::std::boxed::Box<dyn ::std::future::Future<Output=${output}>+'static>>`;
const exchangeError = 'safe_cex_exchange::runtime::ExchangeError';
const incomeVec = applied('alloc::vec::Vec', [path('safe_cex_types::IncomeRecord')]);
const exchangeRaw = future(result({ generic: 'T' }, exchangeError));
const httpRaw = future(result(path('safe_cex_transport::provider::RestExecution'), 'safe_cex_exchange::http::HttpFailure'));
const httpExpanded = future(result(path('safe_cex_transport::provider::RestExecution'), 'safe_cex_transport::provider::RestFailure'));
const fetcher = output => ({ dyn_trait: { traits: [{ trait: { path: 'core::ops::function::FnMut', args: { parenthesized: { inputs: [path('safe_cex_exchange::income_paging::IncomePageParams')], output } } }, generic_params: [] }], lifetime: "'a" } });
const fetcherRaw = fetcher({ resolved_path: { path: 'safe_cex_exchange::runtime::ExchangeFuture', args: { angle_bracketed: { args: [{ lifetime: "'a" }, { type: incomeVec }], constraints: [] } } } });
const sysroot = [['core::pin::Pin','struct'], ['alloc::boxed::Box','struct'], ['core::future::future::Future','trait'], ['core::result::Result','enum']];
const specs = new Map([
 ['safe_cex_exchange::runtime::ExchangeFuture', { ident:'ExchangeFuture', file:'safe-cex-exchange/src/runtime.rs', generics:{params:[lifetime,unbounded],where_predicates:[]}, raw:exchangeRaw, expanded:exchangeRaw, suffix:"<'static,::serde_json::Value>", expected:pinBox('::std::result::Result<::serde_json::Value,crate::runtime::ExchangeError>'), sysroot, workspace:[], kind:'sized-result-future', scope:"one fixed 'static/serde_json::Value actual ExchangeFuture alias; no claim for every lifetime/T or future execution" }],
 ['safe_cex_exchange::http::HttpFuture', { ident:'HttpFuture', file:'safe-cex-exchange/src/http.rs', generics:{params:[lifetime],where_predicates:[]}, raw:httpRaw, expanded:httpExpanded, suffix:"<'static>", expected:pinBox('::std::result::Result<::safe_cex_transport::RestExecution,::safe_cex_transport::RestFailure>'), sysroot, workspace:[['safe_cex_transport::provider::RestExecution','struct','safe-cex-transport'],['safe_cex_transport::provider::RestFailure','struct','safe-cex-transport']], kind:'sized-result-future', scope:"one fixed 'static actual HttpFuture alias; no future execution or response/failure decoder claim" }],
 ['safe_cex_exchange::income_paging::IncomePageFetcher', { ident:'IncomePageFetcher', file:'safe-cex-exchange/src/income_paging.rs', generics:{params:[lifetime],where_predicates:[]}, raw:fetcherRaw, expanded:fetcher(future(result(incomeVec,exchangeError))), suffix:"<'static>", expected:`dyn ::std::ops::FnMut(crate::income_paging::IncomePageParams)->${pinBox('::std::result::Result<::std::vec::Vec<::safe_cex_types::IncomeRecord>,crate::runtime::ExchangeError>')}+'static`, sysroot:[['core::ops::function::FnMut','trait'],...sysroot,['alloc::vec::Vec','struct']], workspace:[['safe_cex_types::IncomeRecord','struct','safe-cex-types']], kind:'unsized-income-fetcher', scope:"one fixed 'static actual unsized IncomePageFetcher callback alias; no callback invocation, polling, income decoding or other lifetime claim" }],
]);

// Only resolved live production context may call this. Synthetic tests exercise
// its refusal logic, not compiler/artifact authentication itself.
export function exchangeFutureAliasProbe(alias, contract, context) {
 const spec=specs.get(alias.name);
 if (!spec) return undefined;
 const label=`${alias.name}: accepted-set assertion: fixed exchange future alias probe`;
 assert.equal(alias.ident,spec.ident,`${label}: alias identifier changed`);
 assert.equal(alias.span?.filename,spec.file,`${label}: compiler source identity changed`);
 assert.deepEqual(alias.generics,spec.generics,`${label}: exact reviewed compiler parameters required`);
 assert.deepEqual(alias.rawTarget,spec.raw,`${label}: source target changed`);
 assert.deepEqual(alias.target,spec.expanded,`${label}: expanded target changed`);
 assert.equal(contract?.category,'non-decoder',`${label}: explicit non-decoder classification required`);
 assert.ok(contract.evidence?.trim()&&contract.reason?.trim(),`${label}: bounded classification evidence required`);
 for (const key of ['instantiation','aliasProbeExpected','aliasProbeKind','aliasProbeSuffix','nativeCanonical']) assert.equal(contract[key],undefined,`${label}: caller cannot supply ${key}`);
 assert.deepEqual(contract.generics,alias.generics,`${label}: compiler generic pin changed`);
 assert.deepEqual(contract.rawTarget,alias.rawTarget,`${label}: compiler source pin changed`);
 assert.deepEqual(contract.target,alias.target,`${label}: compiler expanded pin changed`);
 assert.ok(Array.isArray(context?.macroOrigins)&&Array.isArray(context?.externalTypeAliases),`${label}: actual compiler context required`);
 assert.ok(context.macroOrigins.includes('serde_json'),`${label}: actual serde_json dependency required`);
 for (const [canonical,kind] of spec.sysroot) {
  const rows=context.externalTypeAliases.filter(row=>row.name===alias.name&&row.canonical===canonical);
  assert.ok(rows.length&&rows.every(row=>row.kind===kind&&row.artifact?.kind==='sysroot'),`${label}: actual sysroot ${canonical} identity required`);
 }
 for (const [canonical,kind,packageName] of spec.workspace) {
  const rows=context.externalTypeAliases.filter(row=>row.name===alias.name&&row.canonical===canonical);
  assert.ok(rows.length&&rows.every(row=>row.kind===kind&&row.artifact?.kind==='workspace'&&row.artifact.owner?.name===packageName&&typeof row.artifact.packageId==='string'&&row.artifact.packageId.length&&row.artifact.owner.id===row.artifact.packageId),`${label}: actual workspace ${canonical} identity required`);
 }
 return {suffix:spec.suffix,canonical:spec.kind==='unsized-income-fetcher'?'core::ops::function::FnMut':'core::pin::Pin',expectedType:spec.expected,probeKind:spec.kind,monomorphization:spec.scope};
}
