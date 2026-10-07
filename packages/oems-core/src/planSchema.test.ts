import { OEMS_PLAN_SCHEMA, OEMS_PLAN_SCHEMA_NAME } from './planSchema';

/**
 * Strict mode rejects a schema at REQUEST time, so a rule broken here shows up
 * as a runtime provider error in production rather than as a review comment.
 * Walking the whole tree is cheap insurance against that.
 */
const isObjectSchema = (node: unknown): node is { type: unknown; properties?: Record<string, unknown> } => {
  if (typeof node !== 'object' || node === null) return false;
  const type = (node as { type?: unknown }).type;
  return type === 'object' || (Array.isArray(type) && type.includes('object'));
};

const walk = (node: unknown, path: string, visit: (schema: Record<string, unknown>, path: string) => void): void => {
  if (typeof node !== 'object' || node === null) return;
  if (isObjectSchema(node)) visit(node as Record<string, unknown>, path);

  const properties = (node as { properties?: Record<string, unknown> }).properties;
  if (properties) for (const [key, child] of Object.entries(properties)) walk(child, `${path}.${key}`, visit);

  const items = (node as { items?: unknown }).items;
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
      const keys = Object.keys((schema.properties ?? {}) as Record<string, unknown>).sort();
      const required = [...((schema.required ?? []) as string[])].sort();
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
