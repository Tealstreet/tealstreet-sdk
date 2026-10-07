/**
 * The JSON Schema the provider is held to.
 *
 * ## One flat step object, not an `anyOf` per kind
 *
 * `anyOf` is legal under OpenAI strict mode and would read better here. It is
 * not used because strict mode has no notion of "required given `kind`" — each
 * arm would still list every one of its own fields as required — so the arms
 * buy documentation, not validation, and they buy it at the cost of the model
 * having to pick a branch correctly before it can fill anything in. A flat
 * object with a `kind` enum and nullable fields is the shape models comply
 * with most reliably, and `parseOemsPlan` is where the per-kind requirements
 * are actually enforced.
 *
 * ## Strict mode's two rules, which are easy to break silently
 *
 * Every object needs `additionalProperties: false` AND every one of its
 * properties listed in `required`. There is no optional property: an optional
 * field is a required one whose type includes `"null"`. A schema that breaks
 * either rule is rejected by the API at request time, not at review time,
 * which is what `planSchema.test.ts` walks the whole tree for.
 */

/**
 * ## `reads` is an ADDITIVE field, not a second schema
 *
 * A model that needs data it was not given asks for it here instead of
 * refusing. The alternative — a discriminated union of "plan" and "reads"
 * responses — runs into the same strict-mode limitation as an `anyOf` step:
 * the model must pick a branch before it can fill anything in. One schema with
 * both arrays present, either of which may be empty, is what models comply
 * with most reliably and is the shape the rest of this file already uses.
 *
 * Bump on ANY edit to this schema or to the instructions that ride with it. It
 * is half of the `prompt_cache_key`, so a stale value would serve a plan built
 * against the previous contract.
 */
export const OEMS_PLAN_SCHEMA_VERSION = 2;

/** Must match `^[a-zA-Z0-9_-]+$` — the provider validates the name. */
export const OEMS_PLAN_SCHEMA_NAME = 'tealstreet_oems_plan';

import { READ_CALL_SCHEMA } from './readTools';

const nullableString = { type: ['string', 'null'] } as const;
const nullableNumber = { type: ['number', 'null'] } as const;
const nullableBoolean = { type: ['boolean', 'null'] } as const;
const nullableStringArray = { type: ['array', 'null'], items: { type: 'string' } } as const;

const POSITION_SCOPE = {
  type: ['object', 'null'],
  additionalProperties: false,
  properties: {
    accounts: { ...nullableStringArray, description: 'Account names. Null or empty means every account.' },
    symbols: { ...nullableStringArray, description: 'Symbols. Null or empty means every symbol held.' },
    side: { type: ['string', 'null'], enum: ['long', 'short', null], description: 'One leg only. Null means both.' },
  },
  required: ['accounts', 'symbols', 'side'],
} as const;

const ORDER_SCOPE = {
  type: ['object', 'null'],
  additionalProperties: false,
  properties: {
    accounts: { ...nullableStringArray },
    symbols: { ...nullableStringArray },
    side: { type: ['string', 'null'], enum: ['buy', 'sell', null] },
    reduceOnly: { ...nullableBoolean },
  },
  required: ['accounts', 'symbols', 'side', 'reduceOnly'],
} as const;

const STEP = {
  type: 'object',
  additionalProperties: false,
  properties: {
    kind: {
      type: 'string',
      enum: [
        'reduce_positions',
        'close_positions',
        'cancel_orders',
        'place_order',
        'place_stop_loss',
        'amend_order',
        'set_leverage',
      ],
      description: 'Which action this step asks for. Every other field is read only for the kinds that use it.',
    },
    positionScope: { ...POSITION_SCOPE, description: 'Used by reduce_positions and close_positions.' },
    orderScope: { ...ORDER_SCOPE, description: 'Used by cancel_orders.' },
    percent: { ...nullableNumber, description: 'reduce_positions only. Strictly between 0 and 100.' },
    account: {
      ...nullableString,
      description: 'Required for place_order, place_stop_loss, amend_order, set_leverage.',
    },
    symbol: { ...nullableString, description: 'Required for place_order, place_stop_loss, set_leverage.' },
    side: {
      type: ['string', 'null'],
      enum: ['buy', 'sell', 'long', 'short', null],
      description: 'buy/sell for place_order and place_stop_loss; long/short for set_leverage on a hedged account.',
    },
    orderType: { type: ['string', 'null'], enum: ['market', 'limit', null], description: 'place_order only.' },
    amount: {
      ...nullableNumber,
      description:
        "Size in the venue's own units — the same unit as a position's nativeContracts and an order's nativeRemaining, NOT the base contracts field.",
    },
    price: { ...nullableNumber, description: 'Limit price, or the TRIGGER price for place_stop_loss.' },
    reduceOnly: { ...nullableBoolean, description: 'place_order only.' },
    orderId: { ...nullableString, description: 'amend_order only. Must be an id present in the context.' },
    leverage: { ...nullableNumber, description: 'set_leverage only.' },
  },
  required: [
    'kind',
    'positionScope',
    'orderScope',
    'percent',
    'account',
    'symbol',
    'side',
    'orderType',
    'amount',
    'price',
    'reduceOnly',
    'orderId',
    'leverage',
  ],
} as const;

export const OEMS_PLAN_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  properties: {
    reply: {
      type: 'string',
      description:
        'What to say to the trader, in one or two plain sentences. When you cannot act, this is where you say why or ask for what is missing.',
    },
    steps: {
      type: 'array',
      description: 'The actions to take. Empty when the reply is a question, an answer, or a refusal.',
      items: STEP,
    },
    reads: {
      type: 'array',
      description:
        'Data to fetch before answering. Use this instead of saying you lack information. When this is non-empty, leave steps empty — you will be asked again with the results.',
      items: READ_CALL_SCHEMA,
    },
  },
  required: ['reply', 'steps', 'reads'],
} as const;
