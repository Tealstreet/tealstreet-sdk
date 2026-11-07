import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import {checkLighterModifySigning} from './lighter-modify-signing.mjs';
const module=await import('../hosts/wasm/pkg/web/safe_cex_wasm.js');
await module.default({module_or_path:readFileSync(new URL('../hosts/wasm/pkg/web/safe_cex_wasm_bg.wasm',import.meta.url))});
test('WASM tx17 device signatures match official SDK vectors and refuse field tampering',()=>checkLighterModifySigning(module.WasmEngine));
