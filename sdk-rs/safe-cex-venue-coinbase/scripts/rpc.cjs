// One live-source subprocess for the inert RPC client, HTTP factory and heartbeat operations.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH ?? 'typescript');
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/coinbase/coinbase.starbase.ts');
const source = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const aliases = new Map(source.statements.filter(ts.isTypeAliasDeclaration).map(node => [node.name.text, node]));
const requestAlias = aliases.get('CoinbaseStarbaseJsonRpcRequest');
const requestDeclaration = {
 constraint: requestAlias.typeParameters[0].constraint.getText(source),
 paramsAlias: aliases.get('CoinbaseStarbaseJsonRpcParams').type.getText(source),
 fields: requestAlias.type.members.map(field => ({name:field.name.text, optional:!!field.questionToken, type:field.type.getText(source)}))
};
const exportsObject = {};
vm.runInNewContext(ts.transpileModule(fs.readFileSync(file, 'utf8'), {compilerOptions: {module: ts.ModuleKind.CommonJS}}).outputText, {exports: exportsObject});
const {CoinbaseStarbaseJsonRpcClient: Client, CoinbaseStarbaseHeartbeatController: Heartbeat, CoinbaseStarbaseRpcError: RpcError, createCoinbaseStarbaseHttpTransport: http} = exportsObject;
const error = e => ({name: e.name, message: e.message, ...(e instanceof RpcError ? {code: e.code, method: e.method, ...e.data === undefined ? {} : {data: e.data}, ...e.requestId === undefined ? {} : {requestId: e.requestId}} : {}), display: e.toString()});
const capture = async operation => {try {const value = await operation(); return {ok: value, returnedUndefined: value === undefined}} catch(e) {return {error: error(e)}}};
(async () => {
 const constructors = [];
 for (const initial of [undefined, null, 0, -1, 1.5, '1', false, [], {}, 1, 7, Number.MAX_SAFE_INTEGER, Number.MAX_SAFE_INTEGER + 1]) {
  const result = await capture(() => {const c = new Client(async () => null, {kind: 'http', initialRequestId: initial});return c.buildRequest('public/test', {});});
  constructors.push({...initial === undefined ? {} : {initial}, result});
 }
 const builds = [];
 for (const method of ['public/test', 'private/a_0', 'PUBLIC/test', null, 7]) for (const params of [{marker: 'native-param'}, null, [], 7]) {
  const c = new Client(async () => null, {kind: 'http', initialRequestId: 7});
  const result = await capture(() => c.buildRequest(method, params));
  const next = c.buildRequest('public/test', {});
  builds.push({method, params, result, next});
 }
 const exhausted = new Client(async () => null, {kind: 'http', initialRequestId: Number.MAX_SAFE_INTEGER});
 const exhaustion = [await capture(() => exhausted.buildRequest('public/test', {})), await capture(() => exhausted.buildRequest('public/test', {})), await capture(() => exhausted.buildRequest('invalid', []))];
 const responseInputs = [null, false, [], {}, {jsonrpc: '1.0', id: 7, result: 'wrong'},
  {jsonrpc: '2.0', id: 7, result: null}, {jsonrpc: '2.0', id: 7, result: {price: 'native-price', quantity: 'native-quantity'}}, {jsonrpc: '2.0', id: 7, result: [1, false, null]},
  {jsonrpc: '2.0', result: 'missing-id'}, {jsonrpc: '2.0', id: 8, result: 'mismatch'}, {jsonrpc: '2.0', id: '7', result: 'string-id'},
  {jsonrpc: '2.0', id: 7}, {jsonrpc: '2.0', id: 7, result: null, error: {code: 1, message: 'both'}},
  ...[undefined, null, 8, '7', false, 7].map(id => ({jsonrpc: '2.0', ...id === undefined ? {} : {id}, error: {code: -32001, message: 'native-message', data: {reason: 'native-data'}}})),
  {jsonrpc: '2.0', id: 7, error: {code: -32001, message: ''}}, {jsonrpc: '2.0', id: 7, error: {code: 2, message: 'null-data', data: null}},
  ...[null, [], {code: 1.5, message: 'fractional'}, {code: 1, message: 7}, {message: 'missing-code'}].map(error => ({jsonrpc: '2.0', id: 8, error}))];
 const responses = [];
 for (const response of responseInputs) {
  const calls = [];
  const c = new Client(async (request, context) => {calls.push({request, context});return response;}, {kind: 'http', initialRequestId: 7});
  const result = await capture(() => c.request('private/read', {marker: 'native-param'}, {authorization: 'Bearer native-auth'}));
  responses.push({response, result, calls});
 }
 const httpCases = [];
 for (const kind of ['http', 'websocket']) for (const method of ['private/read', 'invalid']) for (const authorization of [undefined, '', ' Bearer native-auth ']) {
  const calls = [];
  const request = {jsonrpc: '2.0', id: 17, method, params: {marker: 'native-body'}};
  const context = {kind, endpoint: 'https://example.test///', authorization};
  const result = await capture(() => http(async wire => {calls.push(wire);return {transportMarker: 'native-response'};})(request, context));
  httpCases.push({request, context, result, calls});
 }
 const sharedParams = {marker: 'before'};
 const sharedClient = new Client(async () => null, {kind: 'http'});
 const sharedRequest = sharedClient.buildRequest('public/test', sharedParams);
 sharedParams.marker = 'after';
 const parameterAlias = {same: sharedRequest.params === sharedParams, marker: sharedRequest.params.marker};
 const c = new Client(async request => {request.id = 99;request.method = 'private/changed';return {jsonrpc: '2.0', id: 7, result: 'unreachable'};}, {kind: 'http', initialRequestId: 7});
 const requestAlias = await capture(() => c.request('public/test', {}));
 const eagerCalls = [];
 const eagerClient = new Client(async request => {eagerCalls.push(request.id);return {jsonrpc: '2.0', id: request.id, result: request.id};}, {kind: 'http', initialRequestId: 7});
 const pending1 = eagerClient.request('public/test', {}), pending2 = eagerClient.request('public/test', {});
 const eager = {callsBeforeAwait: [...eagerCalls], results: await Promise.all([pending1, pending2])};
 const sentinel = new Error('native transport failure');
 const failedClient = new Client(async () => {throw sentinel}, {kind: 'http', initialRequestId: 7});
 let transportSame = false;
 try {await failedClient.request('public/test', {})}catch(e){transportSame = e === sentinel;}
 const transportFailure = {same: transportSame, nextId: failedClient.buildRequest('public/test', {}).id};
 const rpcErrors = [undefined, null, {reason: 'data-identity'}].flatMap(data => [undefined, 17].flatMap(requestId => ['', 'message-identity'].map(message => {
  const raw = {code: -32001, message, ...data === undefined ? {} : {data}};
  const e = new RpcError(raw, 'private/method-identity', requestId);
  return {raw, ...requestId === undefined ? {} : {requestId}, result: error(e)};
 })));
 const heartbeatConstructors = [];
 for (const kind of ['http', 'websocket']) for (const interval of [0, 9, 10, 10.5, NaN, Infinity]) {
  const c = new Client(async () => null, {kind});
  heartbeatConstructors.push({kind, interval: String(interval), result: await capture(() => {new Heartbeat(c, interval);return true})});
 }
 const heartbeats = [];
 for (const action of ['enable', 'disable', 'heartbeat', 'test_request']) for (const response of ['ok', 'unexpected', null, {version: 'native-version'}]) {
  const calls = [];
  const c = new Client(async (request, context) => {calls.push({request, context});return {jsonrpc: '2.0', id: request.id, result: response}}, {kind: 'websocket', initialRequestId: 7});
  const h = new Heartbeat(c, 10.5);
  const result = await capture(() => action === 'enable' ? h.enable() : action === 'disable' ? h.disable() : h.handleSignal(action));
  heartbeats.push({action, response, result, calls, nextId: c.buildRequest('public/test', {}).id});
 }
 process.stdout.write(JSON.stringify({requestDeclaration, constructors, builds, exhaustion, responses, httpCases, parameterAlias, requestAlias, eager, transportFailure, rpcErrors, heartbeatConstructors, heartbeats}));
})().catch(error => {console.error(error);process.exitCode = 1});
