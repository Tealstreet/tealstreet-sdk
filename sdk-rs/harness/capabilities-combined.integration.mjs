// Same catalogue as isolated families; combined feature union is not standalone proof.
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { checkAdvertisedCommands, constructionProfiles } from './capability-conformance.mjs';
import { checkHyperliquidLocalSigning } from './hyperliquid-local-signing.mjs';
import { checkParadexLocalSigning } from './paradex-local-signing.mjs';

const module = await import('../hosts/wasm/pkg/web/safe_cex_wasm.js');
await module.default({ module_or_path: readFileSync(new URL('../hosts/wasm/pkg/web/safe_cex_wasm_bg.wasm', import.meta.url)) });
const names = readFileSync(new URL('../hosts/wasm/js/venues.mjs', import.meta.url), 'utf8')
  .match(/^\s+'?[a-z-]+'?: '[a-z]+',$/gm)
  .map((line) => line.trim().split(':')[0].replaceAll("'", ''));
for (const venue of names) {
  test(`combined capability ingress: ${venue}`, () => {
    for (const [profile, options] of constructionProfiles(venue).entries()) {
      const proofs = checkAdvertisedCommands(() => new module.WasmEngine(venue, JSON.stringify(options)), `combined/${venue}/profile${profile}`);
      console.log(JSON.stringify({ conformance: `${venue}/${profile}`, ingress: proofs.map(({ field, op, evidence }) => ({ field, op, evidence })) }));
    }
  });
}
test('combined Paradex local Stark signing through EngineHost', async () => {
  await checkParadexLocalSigning(module.WasmEngine);
});
for (const spot of [false, true]) {
  test(`combined Hyperliquid${spot ? 'Spot' : ''} local signing through EngineHost`, async () => {
    await checkHyperliquidLocalSigning(module.WasmEngine, spot);
  });
}
