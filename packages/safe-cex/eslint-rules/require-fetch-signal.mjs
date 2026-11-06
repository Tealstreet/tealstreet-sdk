// Connection-critical code must not wait on the network without a bound: a request
// with no deadline can hold whatever waits on it for the life of the tab. The
// bounded helpers (withTimeout, withDeadline, bootHttp, bootFetch) are the way out.
const BOUNDING_HELPERS = new Set(['withTimeout', 'withDeadline']);
const FETCH_HOSTS = new Set(['globalThis', 'self', 'window']);
const AXIOS_METHODS = new Set(['get', 'post', 'put', 'patch', 'delete', 'head', 'options', 'request']);

const propertyName = (property) => {
  if (property.type !== 'Property' || property.computed) return undefined;
  if (property.key.type === 'Identifier') return property.key.name;
  if (property.key.type === 'Literal') return String(property.key.value);
  return undefined;
};

/** An object literal, or a variable declared as one in scope. */
const objectOf = (node, context) => {
  if (node?.type === 'ObjectExpression') return node;
  if (node?.type !== 'Identifier') return undefined;
  const sourceCode = context.sourceCode || context.getSourceCode();
  for (let scope = sourceCode.getScope(node); scope; scope = scope.upper) {
    const variable = scope.set.get(node.name);
    if (variable) {
      const init = variable.defs[0]?.node?.init;
      return init?.type === 'ObjectExpression' ? init : undefined;
    }
  }
  return undefined;
};

const hasKey = (node, keys, context) =>
  objectOf(node, context)?.properties.some((property) => keys.has(propertyName(property))) ?? false;

const memberName = (member) =>
  member.type === 'MemberExpression' && !member.computed && member.property.type === 'Identifier'
    ? member.property.name
    : undefined;

const isFetch = (callee) =>
  (callee.type === 'Identifier' && callee.name === 'fetch') ||
  (callee.type === 'MemberExpression' &&
    callee.object.type === 'Identifier' &&
    FETCH_HOSTS.has(callee.object.name) &&
    memberName(callee) === 'fetch');

const isAxiosCall = (callee) =>
  (callee.type === 'Identifier' && callee.name === 'axios') ||
  (callee.type === 'MemberExpression' &&
    callee.object.type === 'Identifier' &&
    callee.object.name === 'axios' &&
    AXIOS_METHODS.has(memberName(callee)));

const insideBoundingHelper = (node) => {
  for (let current = node.parent; current; current = current.parent) {
    if (
      current.type === 'CallExpression' &&
      current.callee.type === 'Identifier' &&
      BOUNDING_HELPERS.has(current.callee.name)
    ) {
      return true;
    }
  }
  return false;
};

const SIGNAL = new Set(['signal']);
const TIMEOUT_OR_SIGNAL = new Set(['timeout', 'signal']);
const CONNECTION_TIMEOUT = new Set(['connectionTimeout']);

const requireFetchSignal = {
  meta: {
    type: 'problem',
    docs: { description: 'Require a signal, timeout or deadline on network waits in connection-critical code' },
    schema: [],
    messages: {
      fetch: 'fetch() needs a signal: pass one, or call it through withTimeout, withDeadline or bootFetch.',
      axios: 'axios needs a timeout or signal: set one, or use the bounded bootHttp instance.',
      socket: '{{name}} needs a deadline: give it a connectionTimeout, or open it through withTimeout or withDeadline.',
    },
  },
  create(context) {
    return {
      CallExpression(node) {
        if (isFetch(node.callee)) {
          if (!hasKey(node.arguments[1], SIGNAL, context) && !insideBoundingHelper(node)) {
            context.report({ node, messageId: 'fetch' });
          }
          return;
        }
        if (isAxiosCall(node.callee)) {
          if (
            !node.arguments.some((argument) => hasKey(argument, TIMEOUT_OR_SIGNAL, context)) &&
            !insideBoundingHelper(node)
          ) {
            context.report({ node, messageId: 'axios' });
          }
        }
      },
      NewExpression(node) {
        if (node.callee.type !== 'Identifier') return;
        const { name } = node.callee;
        if (name === 'ReconnectingWebSocket') {
          if (!hasKey(node.arguments[2], CONNECTION_TIMEOUT, context)) {
            context.report({ node, messageId: 'socket', data: { name } });
          }
        } else if (name === 'WebSocket' && !insideBoundingHelper(node)) {
          context.report({ node, messageId: 'socket', data: { name } });
        }
      },
    };
  },
};

export default {
  rules: {
    'require-fetch-signal': requireFetchSignal,
  },
};
