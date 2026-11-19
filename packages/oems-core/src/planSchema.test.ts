import { isRecord, requireArray, requireRecord } from './typeChecks.testHelpers';

import { OEMS_PLAN_SCHEMA, OEMS_PLAN_SCHEMA_NAME } from './planSchema';

/**
 * Strict mode rejects a schema at REQUEST time, so a rule broken here shows up
 * as a runtime provider error in production rather than as a review comment.
 * Walking the whole tree is cheap insurance against that.
 */
const isObjectSchema = (node: unknown): node is Record<string, unknown> => {
  if (!isRecord(node)) return false;
  const type = node.type;
  return type === 'object' || (Array.isArray(type) && type.includes('object'));
};

const walk = (node: unknown, path: string, visit: (schema: Record<string, unknown>, path: string) => void): void => {
  if (!isRecord(node)) return;
  if (isObjectSchema(node)) visit(node, path);

  const properties = node.properties;
  if (properties)
    for (const [key, child] of Object.entries(requireRecord(properties))) walk(child, `${path}.${key}`, visit);

  const items = node.items;
  if (items) walk(items, `${path}[]`, visit);
};

describe('OEMS_PLAN_SCHEMA', () => {
  it('has a provider-legal schema name', () => {
    expect(OEMS_PLAN_SCHEMA_NAME).toMatch(/^[a-zA-Z0-9_-]+$/);
  });

  it('sets additionalProperties:false on every object', () => {
    walk(OEMS_PLAN_SCHEMA, 'root', (schema, path) => {
      expect(`${path}: ${String(schema.additionalProperties)}`).toBe(`${path}: false`);
    });
  });

  it('lists every property in required on every object', () => {
    walk(OEMS_PLAN_SCHEMA, 'root', (schema, path) => {
      const keys = Object.keys(requireRecord(schema.properties ?? {})).sort();
      const required = [...requireArray(schema.required ?? [])].sort();
      expect(`${path}: ${required.join(',')}`).toBe(`${path}: ${keys.join(',')}`);
    });
  });

  it('has an object at the root, as strict mode requires', () => {
    expect(OEMS_PLAN_SCHEMA.type).toBe('object');
  });

  it('spells optionality as a null union rather than omission', () => {
    const step = OEMS_PLAN_SCHEMA.properties.steps.items.properties;
    expect(step.percent.type).toEqual(['number', 'null']);
    expect(step.orderId.type).toEqual(['string', 'null']);
    // `kind` is the discriminator and is the one field that is never null.
    expect(step.kind.type).toBe('string');
  });
});
