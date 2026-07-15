const FETCH_METHODS = new Set(['fetchOrders', 'fetchPositions', 'fetchBalance']);

const getMemberName = (member) => {
  if (member.type !== 'MemberExpression') return undefined;
  if (!member.computed && member.property.type === 'Identifier') return member.property.name;
  if (member.computed && member.property.type === 'Literal' && typeof member.property.value === 'string') {
    return member.property.value;
  }
  return undefined;
};

const isFetchCall = (node) =>
  node.type === 'CallExpression' && node.callee.type === 'MemberExpression' && FETCH_METHODS.has(getMemberName(node.callee));

const isPromiseAllCall = (node) =>
  node.type === 'CallExpression' &&
  node.callee.type === 'MemberExpression' &&
  node.callee.object.type === 'Identifier' &&
  node.callee.object.name === 'Promise' &&
  (getMemberName(node.callee) === 'all' || getMemberName(node.callee) === 'allSettled');

const collectPatternNames = (pattern, names = []) => {
  if (!pattern) return names;
  if (pattern.type === 'Identifier') names.push(pattern.name);
  else if (pattern.type === 'AssignmentPattern') collectPatternNames(pattern.left, names);
  else if (pattern.type === 'RestElement') collectPatternNames(pattern.argument, names);
  else if (pattern.type === 'ArrayPattern') pattern.elements.forEach((element) => collectPatternNames(element, names));
  else if (pattern.type === 'ObjectPattern') {
    pattern.properties.forEach((property) =>
      collectPatternNames(property.type === 'RestElement' ? property.argument : property.value, names),
    );
  }
  return names;
};

const callbackReferencesPattern = (callback, pattern, sourceCode) => {
  const callbackScope = sourceCode.scopeManager.acquire(callback);
  if (!callbackScope) return false;
  const names = new Set(collectPatternNames(pattern));
  return callbackScope.variables.some(
    (variable) =>
      names.has(variable.name) &&
      variable.defs.some((definition) => definition.type === 'Parameter') &&
      variable.references.length > 0,
  );
};

const callbackConsumesValue = (callback, sourceCode) => {
  if (!callback || callback.type === 'SpreadElement') return false;
  if (callback.type !== 'ArrowFunctionExpression' && callback.type !== 'FunctionExpression') return true;
  if (callback.params.length === 0) return false;
  return callbackReferencesPattern(callback, callback.params[0], sourceCode);
};

const callbackConsumesAggregateResult = (callback, resultIndex, sourceCode) => {
  if (
    !callback ||
    (callback.type !== 'ArrowFunctionExpression' && callback.type !== 'FunctionExpression') ||
    callback.params[0]?.type !== 'ArrayPattern'
  ) {
    return false;
  }

  const elements = callback.params[0].elements;
  let selected = elements[resultIndex];
  if (!selected) {
    const restIndex = elements.findIndex((element) => element?.type === 'RestElement');
    if (restIndex < 0 || resultIndex < restIndex) return false;
    selected = elements[restIndex];
  }
  return callbackReferencesPattern(callback, selected, sourceCode);
};

const isTransparentWrapper = (parent, current) =>
  (parent.type === 'AwaitExpression' && parent.argument === current) ||
  (parent.type === 'ChainExpression' && parent.expression === current) ||
  (parent.type === 'UnaryExpression' && parent.operator === 'void' && parent.argument === current) ||
  ((parent.type === 'TSAsExpression' || parent.type === 'TSNonNullExpression' || parent.type === 'TSTypeAssertion') &&
    (parent.expression === current || parent.argument === current));

const isDiscarded = (fetchCall, sourceCode) => {
  let current = fetchCall;
  let aggregateResultIndex;
  while (current.parent) {
    const parent = current.parent;
    if (isTransparentWrapper(parent, current)) {
      current = parent;
      continue;
    }
    if (parent.type === 'ArrayExpression') {
      aggregateResultIndex =
        aggregateResultIndex === undefined ? parent.elements.indexOf(current) : -1;
      current = parent;
      continue;
    }

    if (parent.type === 'MemberExpression' && parent.object === current) {
      const chainedCall = parent.parent;
      if (chainedCall?.type !== 'CallExpression' || chainedCall.callee !== parent) return false;

      const method = getMemberName(parent);
      if (
        method === 'then' &&
        (aggregateResultIndex === undefined
          ? callbackConsumesValue(chainedCall.arguments[0], sourceCode)
          : aggregateResultIndex >= 0 &&
            callbackConsumesAggregateResult(chainedCall.arguments[0], aggregateResultIndex, sourceCode))
      ) {
        return false;
      }
      if (method === 'then' || method === 'catch' || method === 'finally') {
        current = chainedCall;
        continue;
      }
      return false;
    }

    if (parent.type === 'CallExpression' && parent.arguments.includes(current)) {
      if (isPromiseAllCall(parent)) {
        current = parent;
        continue;
      }
      return false;
    }

    if (parent.type === 'ExpressionStatement') return true;
    return false;
  }
  return false;
};

export const noDiscardedFetchResult = {
  meta: {
    type: 'problem',
    docs: {
      description: 'Require exchange reconciliation fetch results to be consumed',
    },
    schema: [],
    messages: {
      discarded:
        '{{method}}() returns reconciliation data; assign, return, or pass the result to a store update instead of discarding it.',
    },
  },
  create(context) {
    const sourceCode = context.sourceCode || context.getSourceCode();

    return {
      CallExpression(node) {
        if (isFetchCall(node) && isDiscarded(node, sourceCode)) {
          context.report({
            node,
            messageId: 'discarded',
            data: { method: getMemberName(node.callee) },
          });
        }
      },
    };
  },
};

export default {
  rules: {
    'no-discarded-fetch-result': noDiscardedFetchResult,
  },
};
