import { readFileSync } from 'node:fs';

import * as engine from '@tealstreet/safe-cex-wasm/web';
import engineAsset from '../../../sdk-rs/hosts/wasm/pkg/web/safe_cex_wasm_bg.wasm' with { type: 'file' };
import { setWasmEngineLoader } from '@tealstreet/safe-cex/src/exchanges/rust/rust.exchange';

// Bun embeds this asset into standalone releases. Its fs API also reads the
// virtual embedded path, so no package install or writable extraction is needed.
let initialization: Promise<typeof engine> | undefined;
export const loadBundledRustEngine = () =>
  (initialization ??= (async () => {
    // wasm-bindgen types describe raw WASM exports; Bun's file import returns a path.
    await engine.default({ module_or_path: readFileSync(engineAsset as unknown as string) });
    return engine;
  })().catch((error) => {
    initialization = undefined;
    throw error;
  }));

setWasmEngineLoader(loadBundledRustEngine);
