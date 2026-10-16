// Check independently authored expectations against actual maintained TS bodies.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { ts, root, values } = require('./source.cjs');
const api = require('./api-source.cjs');
async function main() {
  const { loadContracts } = await import('../../safe-cex-parity/scripts/accepted-contract-files.mjs');
  const contracts = loadContracts();
  const clockFile = ts.createSourceFile('clock.ts', fs.readFileSync(path.join(root,
    'packages/safe-cex/src/exchanges/asterdex/asterdex.virtualClock.ts'), 'utf8'), ts.ScriptTarget.Latest, true);
  const clockClass = clockFile.statements.find(ts.isClassDeclaration);
  const member = clockClass.members.find(node => node.name?.getText(clockFile) === 'parseServerTime');
  assert.ok(member && ts.isArrowFunction(member.initializer));
  const clockCode = ts.transpileModule(`exports.parse = ${member.initializer.getText(clockFile)};`, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  const clock = {};
  new Function('exports', clockCode)(clock);
  const result = (field, fn) => input => {
    try { return { [field]: fn(input) }; }
    catch (error) { return { error: error.name }; }
  };
  const optional = value => value ?? null;
  const adapters = {
    'helpers::is_aster_dex_stp_mode': values.isAsterDEXStpMode,
    'helpers::trim_non_empty': input => optional(values.trimNonEmpty(input)),
    'helpers::is_aster_dex_v3_private_key': values.isAsterDEXV3PrivateKey,
    'helpers::is_aster_dex_address': values.isAsterDEXAddress,
    'helpers::is_aster_dex_main_wallet_error': values.isAsterDEXMainWalletError,
    'helpers::resolve_aster_dex_v3_auth_context': input => {
      const context = values.resolveAsterDEXV3AuthContext(input);
      return { user: optional(context.userAddress), signer: optional(context.signerAddress),
        private: optional(context.signerPrivateKey) };
    },
    'helpers::resolve_aster_dex_api_version': values.resolveAsterDEXApiVersion,
    'helpers::get_aster_dex_fapi_path': input => values.getAsterDEXFapiPath(input.endpoint, input.version),
    'api_helpers::build_query_string': result('text', api.buildQueryString),
    'api_helpers::build_aster_dex_code_query_string': result('text', api.buildAsterDEXCodeQueryString),
    'api_helpers::without_signing_fields': input => {
      const entries = api.withoutSigningFields(input);
      return { keys: Object.keys(entries), entries };
    },
    'api_helpers::build_v3_signing_payload': input => {
      try {
        const payload = api.buildV3SigningPayload(input);
        return { keys: Object.keys(payload), payload };
      } catch (error) { return { error: error.name }; }
    },
    'api_helpers::strip_query_string': api.stripQueryString,
    'api_helpers::is_public_endpoint': api.isPublicEndpoint,
    'api_helpers::is_aster_dex_code_signed_endpoint': input => api.isAsterDEXCodeSignedEndpoint(input.url, input.method),
    'api_helpers::should_suppress_unknown_order_error': result('ok',
      input => api.shouldSuppressUnknownOrderError(input.config, input.code, input.message)),
    'api_helpers::get_aster_dex_error_detail': result('text', input => api.getAsterDEXErrorDetail(input.message, input.code)),
    'clock_helpers::parse_server_time': input => {
      const number = clock.parse(input.value);
      if (number === undefined) return null;
      const bytes = Buffer.alloc(8);
      bytes.writeDoubleBE(number);
      return bytes.toString('hex');
    },
  };
  assert.equal(Object.keys(adapters).length, 18);
  let cases = 0;
  for (const [name, observe] of Object.entries(adapters)) {
    const owner = `safe_cex_venue_asterdex::${name}`;
    assert.equal(contracts[owner]?.category, 'decoder', owner);
    for (const row of contracts[owner].cases) {
      assert.deepEqual(observe(structuredClone(row.input)), row.output,
        `${owner}: actual TS output for ${JSON.stringify(row.input)}`);
      cases++;
    }
  }
  assert.equal(cases, 87);
  console.log(`Aster helper actual source: 18 owners / ${cases} expectations pass (Node ${process.versions.node}, ${process.arch}).`);
}
main().catch(error => { console.error(error); process.exitCode = 1; });
