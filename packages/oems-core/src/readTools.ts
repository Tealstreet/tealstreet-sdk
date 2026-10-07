import type { JsonSchemaField, ReadManifestEntry } from '@tealstreet/script-api-core/src/runtime/readBindings';
import type { OemsContext } from './context.types';

import { AGENT_READ_API_PATHS } from '@tealstreet/script-api-core/src/runtime/runtimeSurface';
import { CORE_READ_MANIFEST } from '@tealstreet/script-api-core/src/runtime/read-manifest';

/**
 * The read tools an agent may call, projected from the generated manifest.
 *
 * ## Why this is not an OpenAI function-tool descriptor
 *
 * `OpenAiJsonClient` posts to the Responses API with
 * `text.format: { type: 'json_schema', strict: true }` and takes an
 * `instructions` string plus ONE `input` string. It has no `tools` parameter
 * and no multi-turn input array. Emitting `{ type: 'function', … }` here would
 * target an API this codebase cannot call.
 *
 * So a tool call is STRUCTURED OUTPUT like everything else: the surface is
 * described in the prompt (`buildReadToolsBlock`), and the model asks for one
 * by filling in a flat object whose schema is `READ_CALL_SCHEMA`.
 *
 * ## The schema is FLAT, and derived
 *
 * Same reasoning as `planSchema.ts`: strict mode has no "required given
 * `tool`", so an `anyOf` arm per tool would buy documentation rather than
 * validation while making the model pick a branch before it can fill anything
 * in. One object with a `tool` enum and nullable fields is what models comply
 * with most reliably.
 *
 * The properties are the UNION of every tool's own parameters, derived from
 * the manifest rather than restated — so adding a read capability updates this
 * schema with no edit here. `assertNoFieldConflict` fails the build if two
 * tools ever give one field name two different base types, which is the single
 * way a derived flat union can go quietly wrong.
 */

export const AGENT_READ_TOOLS: readonly ReadManifestEntry[] = CORE_READ_MANIFEST.filter((entry) =>
  (AGENT_READ_API_PATHS as readonly string[]).includes(entry.path),
);

export const AGENT_READ_TOOL_NAMES: readonly string[] = AGENT_READ_TOOLS.map((entry) => entry.method);

/** Keyed by `string`, not `ReadMethodName`: the lookup key arrives from the
 *  model and is not yet known to be a method name. */
const toolByName = new Map<string, ReadManifestEntry>(AGENT_READ_TOOLS.map((entry) => [entry.method, entry]));

/** A model may ask for several reads at once; this bounds one turn's fan-out. */
export const MAX_READ_CALLS_PER_TURN = 6;

const baseTypeOf = (field: JsonSchemaField): string => {
  const type = field.type;
  const types = Array.isArray(type) ? type : [type];
  return types.filter((entry) => entry !== 'null').join('|');
};

/**
 * Every field nullable, because in a flat union a tool only fills its own.
 *
 * The `enum` has to widen with the type. A field typed `['string','null']`
 * whose enum lists only real values leaves `null` illegal under strict mode,
 * so the model is forced to invent a timeframe on `getBalance`. `planSchema.ts`
 * sets the convention: a nullable enum carries `null` as a member.
 */
const nullable = (field: JsonSchemaField): JsonSchemaField => {
  const type = field.type;
  const types = Array.isArray(type) ? [...type] : [type];
  const widened = types.includes('null') ? types : [...types, 'null'];
  const values = field.enum;
  return {
    ...field,
    type: widened,
    ...(values && !values.includes(null) ? { enum: [...values, null] } : {}),
  };
};

/**
 * Exported so the conflict branch is reachable from a test with a synthetic
 * pair of tools. Called with the real manifest it must never throw — and if it
 * ever does, the build fails at module load, which is the intent.
 */
export const buildReadCallProperties = (
  tools: readonly ReadManifestEntry[],
): Readonly<Record<string, JsonSchemaField>> => {
  const properties: Record<string, JsonSchemaField> = Object.create(null) as Record<string, JsonSchemaField>;
  const seenBase = new Map<string, string>();
  for (const tool of tools) {
    for (const [name, field] of Object.entries(tool.params.properties)) {
      const base = baseTypeOf(field);
      const previous = seenBase.get(name);
      if (previous !== undefined && previous !== base) {
        throw new Error(
          `read tool field "${name}" is ${previous} on one tool and ${base} on ${tool.method}; ` +
            'a flat union cannot express both — rename one of them in readBindings.ts',
        );
      }
      seenBase.set(name, base);
      if (!properties[name]) properties[name] = nullable(field);
    }
  }
  return properties;
};

const READ_CALL_PROPERTIES = buildReadCallProperties(AGENT_READ_TOOLS);

/**
 * One requested read. Nested inside the response schema by the caller.
 *
 * Annotated rather than inferred: spreading a `Record<string, JsonSchemaField>`
 * into an object literal drops the index signature, leaving the inferred type
 * with only `tool` on it and every derived field unreachable to a consumer.
 */
export interface ReadCallSchema {
  readonly type: 'object';
  readonly additionalProperties: false;
  readonly properties: Readonly<Record<string, JsonSchemaField>>;
  readonly required: readonly string[];
}

export const READ_CALL_SCHEMA: ReadCallSchema = Object.freeze({
  type: 'object',
  additionalProperties: false,
  properties: {
    tool: {
      type: 'string',
      enum: AGENT_READ_TOOL_NAMES,
      description: 'Which read to run. Every other field is used only by the tools that declare it.',
    },
    ...READ_CALL_PROPERTIES,
  },
  required: ['tool', ...Object.keys(READ_CALL_PROPERTIES)],
});

/**
 * The surface, as prompt text.
 *
 * Rendered rather than hand-written for the same reason the schema is derived:
 * a tool the model is not told about is a tool it will never call, and a tool
 * described from a stale copy is one it will call wrongly.
 */
export const buildReadToolsBlock = (): string => {
  const lines = AGENT_READ_TOOLS.map((tool) => {
    const params = Object.entries(tool.params.properties).map(([name, field]) => {
      const optional = (Array.isArray(field.type) ? field.type : [field.type]).includes('null');
      const values = field.enum ? ` one of ${field.enum.filter((v) => v !== null).join('/')}` : '';
      return `${name}${optional ? '?' : ''}:${baseTypeOf(field)}${values}`;
    });
    return `- ${tool.method}(${params.join(', ')}) — ${tool.summary}`;
  });
  return lines.join('\n');
};

export interface ResolvedReadCall {
  readonly tool: string;
  readonly args: Readonly<Record<string, unknown>>;
}

export interface ReadCallRejection {
  readonly tool: string;
  readonly reason: string;
}

export interface ReadCallResolution {
  readonly calls: readonly ResolvedReadCall[];
  readonly rejections: readonly ReadCallRejection[];
}

/**
 * Turn what the model asked for into calls that are safe to execute.
 *
 * Three things happen here and each one is load-bearing.
 *
 * **Fields not declared by the named tool are DROPPED**, rather than passed
 * along. The flat union means every field is present on every call, so a model
 * filling `account` on a tool that takes `accounts` would otherwise reach the
 * binding as an unexpected key.
 *
 * **`account` and `accounts` are checked against the snapshot.** This is the
 * important one. The write path is backstopped by `resolvePlan` looking every
 * address up in the context; a read has no such lookup, so without this check a
 * model that named an account outside the snapshot would still execute against
 * live credentials for it. The prompt says to use only what it was shown; this
 * is what makes that true — but only because the CALLER passes the same
 * bounded context it posted. It passed the raw one until 2026-09-02, and the
 * allowlist was then wider than the prompt by every account past
 * `OEMS_CONTEXT_LIMITS.accounts`.
 *
 * **An omitted `accounts` list is substituted, not passed through.**
 * `getCombinedBalance` reads null as "every streaming account" — right for a
 * script the user wrote, wrong for a caller whose reach is meant to be the
 * snapshot — and an absent value never reaches the check above.
 *
 * **The fan-out is capped.** `getCandles` and `getHistoricalPosition` are
 * `network` cost, and an unbounded list of them is an unbounded number of venue
 * round trips per turn.
 */
export const parseReadCalls = (raw: unknown, context: OemsContext): ReadCallResolution => {
  if (!Array.isArray(raw)) return { calls: [], rejections: [] };

  const known = new Set(context.accounts.map((account) => account.name));
  const calls: ResolvedReadCall[] = [];
  const rejections: ReadCallRejection[] = [];
  let skippedForCap = 0;

  for (const entry of raw) {
    // Capped on ACCEPTED calls, not on entries examined: slicing the input
    // first meant six malformed requests consumed the whole turn's budget and
    // a valid seventh was never looked at.
    if (calls.length >= MAX_READ_CALLS_PER_TURN) {
      skippedForCap += 1;
      continue;
    }
    if (typeof entry !== 'object' || entry === null) continue;
    const flat = entry as Record<string, unknown>;
    const name = typeof flat.tool === 'string' ? flat.tool : '';
    const tool = toolByName.get(name);
    if (!tool) {
      rejections.push({ tool: name || '(unnamed)', reason: `${name || 'That'} is not a read this terminal offers.` });
      continue;
    }

    const args: Record<string, unknown> = {};
    for (const key of Object.keys(tool.params.properties)) {
      const value = flat[key];
      if (value !== null && value !== undefined) args[key] = value;
    }

    // A field the tool declares non-nullable is genuinely required. Without
    // this the omission surfaces as a throw from inside the binding at
    // execution time, which reads to the user as a failure rather than as the
    // model having asked for something incomplete.
    const missing = Object.entries(tool.params.properties)
      .filter(([key, field]) => {
        const types = Array.isArray(field.type) ? field.type : [field.type];
        return !types.includes('null') && args[key] === undefined;
      })
      .map(([key]) => key);
    if (missing.length > 0) {
      rejections.push({ tool: name, reason: `${name} needs ${missing.join(' and ')}.` });
      continue;
    }

    // Present-but-not-a-string is rejected outright rather than falling
    // through as `undefined`, which would SKIP the allowlist below and leave
    // the value attached for the binding to throw on. A control documented as
    // the enforcement must not depend on a downstream throw — the same reason
    // `accounts` uses `findIndex` rather than `find`.
    if (args.account !== undefined && typeof args.account !== 'string') {
      rejections.push({ tool: name, reason: 'The account argument must be an account name.' });
      continue;
    }
    if (typeof args.account === 'string' && !known.has(args.account)) {
      rejections.push({ tool: name, reason: `No account named "${args.account}" is connected.` });
      continue;
    }

    if (args.accounts !== undefined && !Array.isArray(args.accounts)) {
      rejections.push({ tool: name, reason: 'The accounts argument must be a list of account names.' });
      continue;
    }
    const accounts = args.accounts as readonly unknown[] | undefined;
    const badIndex = accounts?.findIndex((value) => typeof value !== 'string' || !known.has(value)) ?? -1;
    if (badIndex >= 0) {
      rejections.push({ tool: name, reason: `No account named "${String(accounts?.[badIndex])}" is connected.` });
      continue;
    }

    // An OMITTED account list is not "no restriction" here.
    //
    // `getCombinedBalance` declares `accounts` nullable and its binding reads
    // null as "sum every streaming account" — which is the right default for a
    // script the user wrote, and the wrong one for a caller whose whole
    // account reach is supposed to be the snapshot. Left as null it walked
    // straight past the allowlist above, which only ever runs on values that
    // are present. Substituting the snapshot's own names makes the default
    // mean the same thing the allowlist does.
    if (Object.hasOwn(tool.params.properties, 'accounts') && args.accounts === undefined) {
      args.accounts = [...known];
    }

    calls.push({ tool: name, args });
  }

  if (skippedForCap > 0) {
    rejections.push({
      tool: '(extra)',
      reason: `Only ${MAX_READ_CALLS_PER_TURN} reads run per turn; ${skippedForCap} more were not run.`,
    });
  }

  return { calls, rejections };
};
