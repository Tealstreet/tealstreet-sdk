import { expect, it } from 'vitest';

import { deriveBridgeConstruction } from '../../exchanges/rust/construction';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';

it('Phemex compiled helper declaration agrees with the SDK before engine startup', async () => {
  const { resolveConstruction } = await loadNodeEngine();
  expect(resolveConstruction).toBeTypeOf('function');
  const native = JSON.parse(resolveConstruction!('phemex', '{}'));
  expect(native.bridge).toEqual(deriveBridgeConstruction('phemex', { key: '', secret: '' }));
  expect(native.bridge.helpers).toEqual({ resolveLeverageContext: true });
});
