import type { OemsPositionSide, OemsSide } from './context.types';
import type { OemsIntent, OemsOrderScope, OemsPlan, OemsPositionScope } from './intent.types';

/**
 * Turning the provider's JSON into an `OemsPlan`.
 *
 * Strict structured output is the provider's promise about shape, not ours.
 * It also cannot express the only rules that matter here — which fields a
 * given `kind` requires — so the flat step object arrives with twelve nullable
 * fields and this is where it becomes a discriminated union or an error.
 *
 * ## Keys are OMITTED, never set to `undefined`
 *
 * The host's `updateOrder` decides "did you send price or amount?" with
 * `Object.hasOwn`, so `{price: undefined, amount: 5}` counts as BOTH and is
 * refused. Every optional field here is therefore assigned conditionally.
 *
 * ## The caps are an abuse bound
 *
 * A plan is agent-authored text arriving from a network call. Twenty steps is
 * far more than any sentence a person types produces, and fifty entries per
 * scope list covers a full book; both exist so a malformed or hostile response
 * cannot make the resolver walk something unbounded.
 */

const MAX_STEPS = 20;
const MAX_SCOPE_ENTRIES = 50;
const MAX_STRING_CHARS = 200;

export class OemsPlanError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'OemsPlanError';
  }
}

const fail = (message: string): never => {
  throw new OemsPlanError(message);
};

const record = (value: unknown, where: string): Record<string, unknown> => {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) fail(`${where} must be an object`);
  return value as Record<string, unknown>;
};

/** `null` is how strict mode spells "absent", so both mean the same here. */
const absent = (value: unknown): boolean => value === undefined || value === null;

const str = (value: unknown, where: string): string => {
  if (typeof value !== 'string' || value.length === 0) fail(`${where} must be a non-empty string`);
  const text = value as string;
  if (text.length > MAX_STRING_CHARS) fail(`${where} is longer than ${MAX_STRING_CHARS} characters`);
  return text;
};

const num = (value: unknown, where: string): number => {
  if (typeof value !== 'number' || !Number.isFinite(value)) fail(`${where} must be a finite number`);
  return value as number;
};

const optionalNum = (value: unknown, where: string): number | undefined =>
  absent(value) ? undefined : num(value, where);

const optionalBool = (value: unknown, where: string): boolean | undefined => {
  if (absent(value)) return undefined;
  if (typeof value !== 'boolean') fail(`${where} must be true or false`);
  return value as boolean;
};

const optionalStrings = (value: unknown, where: string): readonly string[] | undefined => {
  if (absent(value)) return undefined;
  if (!Array.isArray(value)) fail(`${where} must be an array of strings`);
  const list = value as unknown[];
  if (list.length > MAX_SCOPE_ENTRIES) fail(`${where} carries more than ${MAX_SCOPE_ENTRIES} entries`);
  return list.map((entry, index) => str(entry, `${where}[${index}]`));
};

const oneOf = <T extends string>(value: unknown, allowed: readonly T[], where: string): T | undefined => {
  if (absent(value)) return undefined;
  if (typeof value !== 'string' || !(allowed as readonly string[]).includes(value)) {
    fail(`${where} must be one of ${allowed.join(', ')}`);
  }
  return value as T;
};

const positionScope = (raw: unknown, where: string): OemsPositionScope => {
  if (absent(raw)) return {};
  const source = record(raw, where);
  const scope: { accounts?: readonly string[]; symbols?: readonly string[]; side?: OemsPositionSide } = {};
  const accounts = optionalStrings(source.accounts, `${where}.accounts`);
  const symbols = optionalStrings(source.symbols, `${where}.symbols`);
  const side = oneOf<OemsPositionSide>(source.side, ['long', 'short'], `${where}.side`);
  if (accounts) scope.accounts = accounts;
  if (symbols) scope.symbols = symbols;
  if (side) scope.side = side;
  return scope;
};

const orderScope = (raw: unknown, where: string): OemsOrderScope => {
  if (absent(raw)) return {};
  const source = record(raw, where);
  const scope: { accounts?: readonly string[]; symbols?: readonly string[]; side?: OemsSide; reduceOnly?: boolean } =
    {};
  const accounts = optionalStrings(source.accounts, `${where}.accounts`);
  const symbols = optionalStrings(source.symbols, `${where}.symbols`);
  const side = oneOf<OemsSide>(source.side, ['buy', 'sell'], `${where}.side`);
  const reduceOnly = optionalBool(source.reduceOnly, `${where}.reduceOnly`);
  if (accounts) scope.accounts = accounts;
  if (symbols) scope.symbols = symbols;
  if (side) scope.side = side;
  if (reduceOnly !== undefined) scope.reduceOnly = reduceOnly;
  return scope;
};

const parseStep = (raw: unknown, where: string): OemsIntent => {
  const source = record(raw, where);
  const kind = str(source.kind, `${where}.kind`);

  switch (kind) {
    case 'reduce_positions':
      return {
        kind,
        scope: positionScope(source.positionScope, `${where}.positionScope`),
        percent: num(source.percent, `${where}.percent`),
      };

    case 'close_positions':
      return { kind, scope: positionScope(source.positionScope, `${where}.positionScope`) };

    case 'cancel_orders':
      return { kind, scope: orderScope(source.orderScope, `${where}.orderScope`) };

    case 'place_order': {
      const step: Extract<OemsIntent, { kind: 'place_order' }> = {
        kind,
        account: str(source.account, `${where}.account`),
        symbol: str(source.symbol, `${where}.symbol`),
        side: oneOf<OemsSide>(source.side, ['buy', 'sell'], `${where}.side`) ?? fail(`${where}.side is required`),
        orderType:
          oneOf<'market' | 'limit'>(source.orderType, ['market', 'limit'], `${where}.orderType`) ??
          fail(`${where}.orderType is required`),
        amount: num(source.amount, `${where}.amount`),
      };
      const price = optionalNum(source.price, `${where}.price`);
      const reduceOnly = optionalBool(source.reduceOnly, `${where}.reduceOnly`);
      return {
        ...step,
        ...(price === undefined ? {} : { price }),
        ...(reduceOnly === undefined ? {} : { reduceOnly }),
      };
    }

    case 'place_stop_loss':
      return {
        kind,
        account: str(source.account, `${where}.account`),
        symbol: str(source.symbol, `${where}.symbol`),
        side: oneOf<OemsSide>(source.side, ['buy', 'sell'], `${where}.side`) ?? fail(`${where}.side is required`),
        amount: num(source.amount, `${where}.amount`),
        price: num(source.price, `${where}.price`),
      };

    case 'amend_order': {
      const price = optionalNum(source.price, `${where}.price`);
      const amount = optionalNum(source.amount, `${where}.amount`);
      if ((price === undefined) === (amount === undefined)) {
        fail(`${where} must change exactly one of price or amount`);
      }
      return {
        kind,
        account: str(source.account, `${where}.account`),
        orderId: str(source.orderId, `${where}.orderId`),
        ...(price === undefined ? {} : { price }),
        ...(amount === undefined ? {} : { amount }),
      };
    }

    case 'set_leverage': {
      const side = oneOf<OemsPositionSide>(source.side, ['long', 'short'], `${where}.side`);
      return {
        kind,
        account: str(source.account, `${where}.account`),
        symbol: str(source.symbol, `${where}.symbol`),
        leverage: num(source.leverage, `${where}.leverage`),
        ...(side === undefined ? {} : { side }),
      };
    }

    default:
      return fail(`${where}.kind "${kind}" is not an action this build knows`);
  }
};

/**
 * Reads stay RAW. Validating one needs the context snapshot — which account
 * names exist — and this function does not take a context. `parseReadCalls`
 * does that at the point of use, in the browser about to run them against live
 * credentials.
 */
const readsOf = (source: Record<string, unknown>): readonly unknown[] => {
  if (absent(source.reads)) return [];
  if (!Array.isArray(source.reads)) fail('plan.reads must be an array');
  return source.reads as unknown[];
};

export const parseOemsPlan = (raw: unknown): OemsPlan => {
  const source = record(raw, 'plan');

  const reply = typeof source.reply === 'string' ? source.reply.slice(0, 2_000) : '';
  const reads = readsOf(source);

  // A plan that asks for data AND proposes actions computed its actions
  // WITHOUT that data. Gathering wins and the steps are dropped: executing
  // them would be acting on the model's own admission that it was reasoning
  // with something missing. The next hop proposes again, with the answer.
  if (reads.length > 0) return { reply, steps: [], reads };

  if (absent(source.steps)) return { reply, steps: [], reads };
  if (!Array.isArray(source.steps)) fail('plan.steps must be an array');

  const steps = source.steps as unknown[];
  if (steps.length > MAX_STEPS) fail(`plan carries more than ${MAX_STEPS} steps`);

  return { reply, steps: steps.map((step, index) => parseStep(step, `plan.steps[${index}]`)), reads };
};
