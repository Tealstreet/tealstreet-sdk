import type { NativeAmountMarket } from '@tealstreet/safe-cex/src/utils/nativeAmountRules';
import {
  nativeAmountRules as sdkRules,
  assertNativeReduceAmount as sdkAssert,
} from '@tealstreet/safe-cex/src/utils/nativeAmountRules';
import { ScriptValidationError } from './ScriptValidationError';

export type { NativeAmountMarket } from '@tealstreet/safe-cex/src/utils/nativeAmountRules';
export { sameNativeAmount } from '@tealstreet/safe-cex/src/utils/nativeAmountRules';

// Adapt the SDK unit rules to the portable order policy's refusal type.
const validate = <T>(run: () => T): T => {
  try {
    return run();
  } catch (error) {
    throw new ScriptValidationError(error instanceof Error ? error.message : String(error));
  }
};
export const nativeAmountRules = (market: NativeAmountMarket, exchangeName?: string) =>
  validate(() => sdkRules(market, exchangeName));
export const assertNativeReduceAmount = (amount: number, market: NativeAmountMarket, exchangeName?: string): void =>
  validate(() => sdkAssert(amount, market, exchangeName));
