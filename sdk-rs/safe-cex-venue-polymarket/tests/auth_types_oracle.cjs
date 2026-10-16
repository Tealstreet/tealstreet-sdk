// Executes repository TS under a recording HTTP boundary; no network or wallet key.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { createRequire } = require('node:module');
const compilerPath = process.env.SAFE_CEX_TYPESCRIPT_PATH;
assert(compilerPath, 'SAFE_CEX_TYPESCRIPT_PATH is required');
const ts = require(compilerPath);
const deps = createRequire(path.join(compilerPath, 'package.json'));
const sourceDir = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/polymarket');
const typePath = path.join(sourceDir, 'polymarket.types.ts');
const selected = ['PolymarketCredentials', 'PolymarketDeriveKeyRequest', 'PolymarketDeriveKeyResponse', 'PolymarketL2Headers', 'PolymarketBalanceAllowance', 'PolymarketOrderBookLevel', 'PolymarketPrice', 'PolymarketOrderBook', 'PolymarketOrderResponse', 'PolymarketClobMarket'];

// Resolve properties through the checker, including heritage, never members alone.
function inventory(fileName, source) {
  const options = { strict: true, noLib: true, noResolve: true };
  const host = ts.createCompilerHost(options);
  // Actual compiler Array declarations let the checker resolve nested element types.
  const library = path.join(path.dirname(ts.getDefaultLibFilePath(options)), 'lib.es5.d.ts');
  const libraryText = fs.readFileSync(library, 'utf8');
  host.getSourceFile = (file, language) => file === fileName
    ? ts.createSourceFile(file, source, language, true)
    : file === library ? ts.createSourceFile(file, libraryText, language, true) : undefined;
  const program = ts.createProgram([fileName, library], options, host);
  const checker = program.getTypeChecker();
  const file = program.getSourceFile(fileName);
  const result = {};
  for (const node of file.statements) {
    if (!ts.isInterfaceDeclaration(node)) continue;
    const type = checker.getTypeAtLocation(node);
    const expectedBases = node.heritageClauses?.flatMap(clause => [...clause.types]).length ?? 0;
    const actualBases = checker.getBaseTypes(type);
    assert.equal(actualBases.length, expectedBases, `resolved_heritage ${node.name.text}`);
    assert.equal(checker.getIndexInfosOfType(type).length, 0, `index signature ${node.name.text}`);
    assert.equal(type.getCallSignatures().length, 0, `call signature ${node.name.text}`);
    assert.equal(type.getConstructSignatures().length, 0, `construct signature ${node.name.text}`);
    result[node.name.text] = type.getProperties().map(property => {
      const declarations = property.getDeclarations();
      assert(declarations?.every(ts.isPropertySignature), `unsupported property ${property.name}`);
      const optional = !!(property.flags & ts.SymbolFlags.Optional);
      const valueType = checker.getTypeOfSymbolAtLocation(property, declarations[0]);
      const arms = valueType.isUnion() ? valueType.types : [valueType];
      const meaningful = arms.filter(arm => !(arm.flags & ts.TypeFlags.Undefined));
      if (meaningful.length === 2 && meaningful.every(arm => arm.flags & ts.TypeFlags.BooleanLiteral)) {
        return [property.name, optional, 'boolean'];
      }
      if (meaningful.length !== 1) return [property.name, optional, 'unsupported'];
      if (checker.isArrayType(meaningful[0])) {
        const element = checker.getTypeArguments(meaningful[0])[0];
        const name = element.flags & ts.TypeFlags.String ? 'string' : element.getSymbol()?.name;
        return [property.name, optional, name ? name + '[]' : 'unsupported'];
      }
      const primitive = meaningful[0].flags & ts.TypeFlags.String ? 'string'
        : meaningful[0].flags & ts.TypeFlags.Number ? 'number' : 'unsupported';
      return [property.name, optional, primitive];
    });
  }
  return result;
}
const schema = inventory(typePath, fs.readFileSync(typePath, 'utf8'));
function validate(shape, value) {
  assert(value !== null && typeof value === 'object' && !Array.isArray(value), 'record object');
  for (const [field, optional, primitive] of shape) {
    assert.notEqual(primitive, 'unsupported', `unsupported field ${field}`);
    if (optional && value[field] === undefined) continue;
    if (primitive.endsWith('[]')) {
      assert(Array.isArray(value[field]), `declared array ${field}`);
      const name = primitive.slice(0, -2);
      if (name === 'string') {
        for (const item of value[field]) assert.equal(typeof item, 'string', `declared array string ${field}`);
      } else {
        const element = schema[name];
        assert(element, `resolved array element ${field}`);
        for (const item of value[field]) validate(element, item);
      }
    } else {
      assert.equal(typeof value[field], primitive, `declared field ${field}`);
    }
  }
}
const inherited = inventory('heritage-control.ts', 'interface Parent { inherited: string } interface Child extends Parent { local: number }');
assert.deepEqual(inherited.Child, [['local', false, 'number'], ['inherited', false, 'string']], 'inherited_required_inventory');
assert.throws(() => validate(inherited.Child, { local: 7 }), /declared field inherited/, 'inherited_required_refusal');
validate(inherited.Child, { local: 7, inherited: 'present' });
assert.throws(() => inventory('missing-parent.ts', 'interface Child extends MissingParent { local: number }'),
  /resolved_heritage Child/, 'unresolved_heritage_refusal');

const cache = new Map();
function load(name) {
  if (cache.has(name)) return cache.get(name);
  const file = path.join(sourceDir, name + '.ts');
  const text = fs.readFileSync(file, 'utf8');
  const compiled = ts.transpileModule(text, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
  const exports = {};
  function sourceRequire(id) {
    if (id.startsWith('./polymarket.')) return load(id.slice(2));
    if (id === 'ethers' || id === 'qs' || id === 'axios-retry') return deps(id);
    // Only constants use these enum values; no exchange/core behavior is stubbed.
    if (id === '../../types') return { OrderSide: { Buy: 'buy', Sell: 'sell' }, OrderType: { Limit: 'limit', Market: 'market' } };
    if (id === '@polymarket/clob-client-v2' || id === '../../safeCexAxiosCreate' || id === '../../utils/retryReadsOnly') {
      return new Proxy({}, { get() { throw new Error(`unexercised factory dependency ${id}`); } });
    }
    throw new Error(`unconfigured source import ${id}`);
  }
  vm.runInNewContext('(function(require,exports){' + compiled + '\n})', {}, { filename: file })(sourceRequire, exports);
  cache.set(name, exports);
  return exports;
}
const bits = value => {
  const buffer = Buffer.alloc(8);
  buffer.writeDoubleBE(Number.isNaN(value) ? NaN : value);
  return Number.isNaN(value) ? '7ff8000000000000' : buffer.toString('hex');
};
async function main() {
  const credentials = { apiKey: 'credential-key-A', secret: 'c2VjcmV0LWItMQ==', passphrase: 'passphrase-C' };
  const request = { address: 'wallet-D', nonce: 17.25, signature: 'signature-E', timestamp: 1700000123.5 };
  validate(schema.PolymarketCredentials, credentials);
  validate(schema.PolymarketDeriveKeyRequest, request);
  const { PolymarketAPI } = load('polymarket.api');
  const api = Object.create(PolymarketAPI.prototype);
  let recorded;
  let reply = credentials;
  api.xhr = { get: async (url, config) => { recorded = { url, ...config }; return { data: reply }; } };
  const response = await api.deriveApiKey(request.address, request.nonce, request.signature, request.timestamp);
  assert.equal(response, credentials, 'actual_derive_returns_response_identity');
  validate(schema.PolymarketDeriveKeyResponse, response);
  assert.equal(recorded.url, '/auth/derive-api-key', 'actual_derive_endpoint');
  assert.deepEqual(JSON.parse(JSON.stringify(recorded.headers)), {
    POLY_ADDRESS: 'wallet-D', POLY_SIGNATURE: 'signature-E', POLY_TIMESTAMP: '1700000123.5', POLY_NONCE: '17.25',
  }, 'actual_derive_native_header_fields');
  const headers = load('polymarket.signing').buildL2Headers(credentials, 'wallet-F', 'get', '/data/orders', undefined, 1700000456);
  validate(schema.PolymarketL2Headers, headers);
  assert.equal(headers.POLY_API_KEY, credentials.apiKey, 'actual_L2_api_key');
  assert.equal(headers.POLY_PASSPHRASE, credentials.passphrase, 'actual_L2_passphrase');
  const optionalAbsent = { ...headers };
  delete optionalAbsent.POLY_PASSPHRASE;
  validate(schema.PolymarketL2Headers, optionalAbsent);
  assert.throws(() => validate(schema.PolymarketL2Headers, { ...headers, POLY_PASSPHRASE: null }), /declared field/, 'optional_null_refusal');
  const numbers = ['-0', '0', '9007199254740993', '1e400', '-1e400', '1e-400', '-1e-400', '0.10000000000000001', '5e-324'];
  const numeric = [];
  for (const [index, token] of numbers.entries()) {
    const raw = `{"address":"numeric-wallet","nonce":${token},"signature":"numeric-signature","timestamp":${numbers[(index + 3) % numbers.length]}}`;
    const input = JSON.parse(raw);
    validate(schema.PolymarketDeriveKeyRequest, input);
    assert.equal(typeof input.nonce, 'number', 'nonce_type_before_json');
    assert.equal(typeof input.timestamp, 'number', 'timestamp_type_before_json');
    await api.deriveApiKey(input.address, input.nonce, input.signature, input.timestamp);
    assert.equal(recorded.headers.POLY_NONCE, String(input.nonce), 'actual_numeric_nonce');
    assert.equal(recorded.headers.POLY_TIMESTAMP, String(input.timestamp), 'actual_numeric_timestamp');
    numeric.push({ raw, nonceBits: bits(input.nonce), timestampBits: bits(input.timestamp) });
  }
  const stringBoundary = [];
  const strings = [
    ['"scalar"', true], ['"é"', true], ['"e\\u0301"', true],
    ['"\\ud83d\\ude00"', true], ['"\\ufeff x\\t"', true],
    ['"\\ud800"', false], ['"\\udfff"', false], ['"x\\ud800y"', false],
  ];
  for (const [token, accepted] of strings) {
    const value = JSON.parse(token);
    assert.equal(typeof value, 'string', 'string_type_before_json');
    if (!accepted) {
      assert([...Array(value.length).keys()].some(i => value.charCodeAt(i) >= 0xd800 && value.charCodeAt(i) <= 0xdfff),
        'source_lone_surrogate_before_json');
    }
    reply = { ...credentials, apiKey: value };
    const returned = await api.deriveApiKey(request.address, request.nonce, request.signature, request.timestamp);
    assert.equal(returned.apiKey, value, 'source_credential_string_identity');
    const stringRequest = { ...request, address: value };
    await api.deriveApiKey(stringRequest.address, stringRequest.nonce, stringRequest.signature, stringRequest.timestamp);
    assert.equal(recorded.headers.POLY_ADDRESS, value, 'source_request_string_identity');
    const stringHeaders = load('polymarket.signing').buildL2Headers(credentials, value, 'GET', '/data/orders', undefined, 1700000456);
    assert.equal(stringHeaders.POLY_ADDRESS, value, 'source_header_string_identity');
    for (const [name, record] of [
      ['PolymarketCredentials', returned], ['PolymarketDeriveKeyResponse', returned],
      ['PolymarketDeriveKeyRequest', stringRequest], ['PolymarketL2Headers', stringHeaders],
    ]) {
      validate(schema[name], record);
      stringBoundary.push({ name, raw: JSON.stringify(record), accepted });
    }
  }
  // These are wire strings, including nonnumeric spellings: prediction-core owns parsing.
  const payloadCases = [];
  const payloadStrings = ['"9007199254740993"', '"0.00000100"', '""', '"NaN"', '"é"', '"\\ud83d\\ude00"', '"\\ufeff x\\t"', '"\\ud800"', '"x\\udfffy"'];
  for (const [index, token] of payloadStrings.entries()) {
    const value = JSON.parse(token);
    const accepted = index < 7;
    // Exercise every field independently so equal or swapped fields cannot hide.
    for (const field of ['balance', 'allowance']) {
      reply = { balance: 'balance-A', allowance: 'allowance-B', [field]: value };
      const returned = await api.getBalanceAllowance(2);
      assert.equal(recorded.url, '/balance-allowance', 'actual_balance_endpoint');
      assert.equal(recorded.params.asset_type, 'COLLATERAL', 'actual_balance_asset_type');
      assert.equal(recorded.params.signature_type, 2, 'actual_balance_signature_type');
      assert.equal(returned, reply, 'actual_balance_response_identity');
      assert.equal(returned[field], value, 'actual_balance_string_identity');
      validate(schema.PolymarketBalanceAllowance, returned);
      payloadCases.push({ name: 'PolymarketBalanceAllowance', raw: JSON.stringify(returned), accepted });
    }
    for (const field of ['price', 'size']) {
      const level = { price: 'price-C', size: 'size-D', [field]: value };
      reply = { asset_id: 'token-E', bids: [level], asks: [] };
      const returned = await api.getOrderBook('token-E');
      assert.equal(recorded.url, '/book', 'actual_book_endpoint');
      assert.equal(recorded.params.token_id, 'token-E', 'actual_book_token');
      assert.equal(returned, reply, 'actual_book_response_identity');
      assert.equal(returned.bids[0][field], value, 'actual_level_string_identity');
      validate(schema.PolymarketOrderBookLevel, returned.bids[0]);
      payloadCases.push({ name: 'PolymarketOrderBookLevel', raw: JSON.stringify(returned.bids[0]), accepted });
    }
  }
  const priceCases = [];
  const priceNumeric = [];
  async function actualPrice(record) {
    validate(schema.PolymarketPrice, record);
    reply = [record];
    const result = await api.getPrices(['token-A', 'token-B']);
    assert.equal(result, reply, 'actual_prices_response_identity');
    assert.equal(result[0], record, 'actual_price_record_identity');
    assert.equal(recorded.url, '/prices', 'actual_prices_endpoint');
    assert.equal(recorded.params.token_ids, 'token-A,token-B', 'actual_prices_params');
    return result[0];
  }
  const priceSample = await actualPrice({ token_id: 'token-price-A', price: '0.031400', timestamp: 17.125 });
  for (const [index, token] of payloadStrings.entries()) {
    for (const field of ['token_id', 'price']) {
      const record = await actualPrice({ token_id: 'token-price-A', price: 'price-B', [field]: JSON.parse(token) });
      priceCases.push({ raw: JSON.stringify(record), accepted: index < 7 });
    }
  }
  for (const token of numbers) {
    const raw = `{"token_id":"numeric-token","price":"0.012300","timestamp":${token}}`;
    const record = await actualPrice(JSON.parse(raw));
    assert.equal(typeof record.timestamp, 'number', 'price_timestamp_type_before_json');
    priceNumeric.push({ raw, bits: bits(record.timestamp) });
  }
  const bookSample = {
    asset_id: 'book-token-A',
    bids: [{ price: '0.123400', size: '13.25' }, { price: '0.120000', size: '27.50' }],
    asks: [{ price: '0.234500', size: '41.75' }, { price: '0.240000', size: '59.00' }],
    timestamp: 123.625, hash: 'book-hash-B',
  };
  async function actualBook(record) {
    validate(schema.PolymarketOrderBook, record);
    reply = record;
    const returned = await api.getOrderBook(record.asset_id);
    assert.equal(recorded.url, '/book', 'actual_full_book_endpoint');
    assert.equal(recorded.params.token_id, record.asset_id, 'actual_full_book_token');
    assert.equal(returned, record, 'actual_full_book_response_identity');
    return returned;
  }
  await actualBook(bookSample);
  const bookCases = [];
  const bookNumeric = [];
  for (const [index, token] of payloadStrings.entries()) {
    for (const field of ['asset_id', 'hash', 'bids.price', 'bids.size', 'asks.price', 'asks.size']) {
      const record = JSON.parse(JSON.stringify(bookSample));
      const value = JSON.parse(token);
      if (field.includes('.')) {
        const [side, key] = field.split('.');
        record[side][0][key] = value;
      } else record[field] = value;
      bookCases.push({ raw: JSON.stringify(await actualBook(record)), accepted: index < 7 });
    }
  }
  for (const token of numbers) {
    const raw = `{"asset_id":"numeric-book","bids":[{"price":"0.10","size":"1.20"}],"asks":[{"price":"0.30","size":"4.50"}],"timestamp":${token}}`;
    const record = await actualBook(JSON.parse(raw));
    assert.equal(typeof record.timestamp, 'number', 'book_timestamp_type_before_json');
    bookNumeric.push({ raw, bits: bits(record.timestamp) });
  }
  const bookAbsent = await actualBook({ asset_id: 'empty-book', bids: [], asks: [] });
  // Actual batch method returns the typed response; transport is recording-only.
  api.getCredentials = () => credentials;
  api.xhr.post = async (url, payload) => { recorded = { url, payload }; return { data: reply }; };
  const signedOrder = { salt: '17', maker: 'maker-A', signer: 'signer-B', tokenId: 'token-C', makerAmount: '19', takerAmount: '23', expiration: '0', side: 'BUY', signatureType: 0, timestamp: '31', metadata: 'meta-D', builder: 'builder-E', signature: 'signature-F' };
  async function actualOrderResponse(record) {
    validate(schema.PolymarketOrderResponse, record);
    reply = [record];
    const result = await api.postOrders([{ order: signedOrder, orderType: 'GTC' }]);
    assert.equal(recorded.url, '/orders', 'actual_batch_order_endpoint');
    assert.equal(recorded.payload.orders[0].order, signedOrder, 'actual_batch_signed_order_identity');
    assert.equal(recorded.payload.orders[0].owner, credentials.apiKey, 'actual_batch_order_owner');
    assert.equal(recorded.payload.orders[0].orderType, 'GTC', 'actual_batch_order_type');
    assert.equal(result, reply, 'actual_batch_response_identity');
    assert.equal(result[0], record, 'actual_order_response_identity');
    return result[0];
  }
  const orderResponseSample = await actualOrderResponse({ success: true, errorMsg: 'error-A', orderID: 'order-B', transactionsHashes: ['hash-C', 'hash-D'], status: 'status-E', takingAmount: '0.012300', makingAmount: '7.45600' });
  const orderResponseAbsent = await actualOrderResponse({ success: false });
  const orderResponseEmptyHashes = await actualOrderResponse({ success: true, transactionsHashes: [] });
  const orderResponseCases = [];
  for (const [index, token] of payloadStrings.entries()) {
    for (const field of ['errorMsg', 'orderID', 'transactionsHashes', 'status', 'takingAmount', 'makingAmount']) {
      const record = JSON.parse(JSON.stringify(orderResponseSample));
      record.success = index % 2 === 0;
      if (field === 'transactionsHashes') record[field][0] = JSON.parse(token);
      else record[field] = JSON.parse(token);
      orderResponseCases.push({ raw: JSON.stringify(await actualOrderResponse(record)), accepted: index < 7 });
    }
  }
  const capture = { architecture: process.arch, nodeMajor: Number(process.versions.node.split('.')[0]) };
  const clobSample = [
    { condition_id: 'condition-A', token_id: 'token-B', outcome: 'Yes', minimum_order_size: '13.2500', minimum_tick_size: '0.000100', active: true, closed: false },
    { condition_id: 'condition-C', token_id: 'token-D', outcome: 'No', minimum_order_size: '27.5000', minimum_tick_size: '0.010000', active: false, closed: true },
  ];
  async function actualClob(records) {
    for (const record of records) validate(schema.PolymarketClobMarket, record);
    reply = records;
    const result = await api.getMarkets();
    assert.equal(recorded.url, '/markets', 'actual_clob_markets_endpoint');
    assert.equal(result, records, 'actual_clob_markets_response_identity');
    assert.equal(result.length, records.length, 'actual_clob_markets_count');
    return result;
  }
  await actualClob(clobSample);
  const clobCases = [];
  for (const [index, token] of payloadStrings.entries()) {
    for (const field of ['condition_id', 'token_id', 'outcome', 'minimum_order_size', 'minimum_tick_size']) {
      const record = { ...clobSample[0], [field]: JSON.parse(token) };
      const returned = await actualClob([record]);
      clobCases.push({ raw: JSON.stringify(returned[0]), accepted: index < 7 });
    }
  }
  const clobFlagCases = [];
  for (const active of [false, true]) {
    for (const closed of [false, true]) {
      const record = { ...clobSample[0], active, closed };
      clobFlagCases.push((await actualClob([record]))[0]);
    }
  }
  process.stdout.write(JSON.stringify({ capture, clobSample, clobCases, clobFlagCases, orderResponseSample, orderResponseAbsent, orderResponseEmptyHashes, orderResponseCases, bookSample, bookCases, bookNumeric, bookAbsent, priceSample, priceCases, priceNumeric, payloadCases, stringBoundary, schema: Object.fromEntries(selected.map(name => [name, schema[name]])), credentials, request, response, headers, optionalAbsent, numeric }));
}
main().catch(error => { console.error(error); process.exitCode = 1; });
