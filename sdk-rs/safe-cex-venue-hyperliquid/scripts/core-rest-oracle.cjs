'use strict';
const fs = require('node:fs'),
    path = require('node:path'),
    vm = require('node:vm'),
    assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript'),
    root = path.resolve(__dirname, '../../..');
const metadata = require('./core-rest-generate.cjs');
const sourceNames = {
    core: metadata.roots.filter((r) => r[0] === 'core').map((r) => r[1]),
    private: metadata.roots.filter((r) => r[0] === 'private').map((r) => r[1]),
};
const names = metadata.roots.map((r) => r.join('.'));
let g, ids, tags, nextId, calls, protos;
let activeCalls = [],
    owner = 0;
const units = (s) => Array.from({ length: s.length }, (_, i) => s.charCodeAt(i)),
    tag = (v, t) => (tags.set(v, t), v);
function native(v) {
    if (v === undefined) return ['undefined'];
    if (v === null) return ['null'];
    if (typeof v === 'boolean') return ['bool', v];
    if (typeof v === 'string') return ['string', units(v)];
    if (typeof v === 'number') {
        const b = Buffer.alloc(8);
        b.writeDoubleBE(v);
        return ['number', Number.isNaN(v) ? 'nan' : b.toString('hex')];
    }
    if (!ids.has(v)) ids.set(v, nextId++);
    return ['ref', ids.get(v), tags.get(v) || typeof v, typeof v];
}
function op(name, args, fn) {
    const e = { op: name, args };
    calls.push(e);
    try {
        const r = fn();
        e.result = native(r);
        return r;
    } catch (error) {
        e.throw = native(error);
        throw error;
    }
}
function tree(v) {
    if (tags.get(v) === 'map')
        return ['map', native(v), native(Object.getPrototypeOf(v)), Array.from(v, ([k, v]) => [native(k), tree(v)])];
    if (tags.get(v) === 'array')
        return [
            'array',
            native(v),
            native(Object.getPrototypeOf(v)),
            Array.from({ length: v.length }, (_, i) => tree(v[i])),
        ];
    if (tags.get(v) === 'literal')
        return [
            'literal',
            native(v),
            native(Object.getPrototypeOf(v)),
            Object.keys(v).map((k) => [units(k), tree(v[k])]),
        ];
    return native(v);
}
const globals = {};
const bind = {
    __await: async (value) => {
        const e = { op: 'await', args: { value: native(value) } };
        calls.push(e);
        try {
            const result = await value;
            e.result = native(result);
            g.afterAwait();
            e.state = g.state();
            return result;
        } catch (error) {
            e.throw = native(error);
            g.afterAwait();
            e.state = g.state();
            throw error;
        }
    },
    __returnAwait: async (value) => {
        const e = { op: 'returnAwait', args: { value: native(value) } };
        calls.push(e);
        try {
            const result = await value;
            g.afterAwait();
            e.result = native(result);
            e.state = g.state();
            return result;
        } catch (error) {
            g.afterAwait();
            e.throw = native(error);
            e.state = g.state();
            throw error;
        }
    },
    __binding: (object, names) => names.map((n) => bind.__get(object, n)),
    __appendSpread: (array, value) => {
        for (const entry of bind.__iterate(value)) bind.__arrayDefine(array, array.length, entry);
    },
    __unary: (operator, value) =>
        op('positionUnary', { operator, value: native(value) }, () => (operator === 'plus' ? +value : -value)),
    __compare: (operator, left, right) =>
        op('compare', { operator, left: native(left), right: native(right) }, () =>
            operator === 'less' ? left < right : left >= right,
        ),
    __construct: (constructor, ...args) => {
        const value = op('construct', { constructor: native(constructor), args: args.map(native) }, () => {
            if (g.throwConstruct) throw g.error;
            return tag(
                Reflect.construct(constructor, args),
                constructor === Map ? 'map' : constructor === Set ? 'set' : 'error',
            );
        });
        calls.at(-1).prototype = native(Object.getPrototypeOf(value));
        return value;
    },
    __instanceof: (value, constructor) =>
        op('instanceof', { value: native(value), constructor: native(constructor) }, () => {
            if (g.throwInstanceof) throw g.error;
            return value instanceof constructor;
        }),
    __global: (name) =>
        op('global', { name }, () => {
            if (g.throwGlobal === name) throw g.error;
            return globals[name];
        }),
    __get: (object, name) =>
        op('get', { object: native(object), name }, () => {
            if (g.throwGet === name) throw g.error;
            if (object == null) throw new TypeError('nullish get');
            const v = Reflect.get(Object(object), name, object);
            if (typeof v === 'function') tag(v, name);
            return v;
        }),
    __optionalGet: (object, name) => (object == null ? undefined : bind.__get(object, name)),
    __optionalIndex: (object, key) => (object == null ? undefined : bind.__index(object, key)),
    __index: (object, key) =>
        op('index', { object: native(object), key: native(key) }, () => {
            if (object == null) throw new TypeError('nullish index');
            return Reflect.get(Object(object), key, object);
        }),
    __assign: (object, name, value) =>
        op('set', { object: native(object), name, value: native(value) }, () => {
            if (g.throwSet === name) throw g.error;
            object[name] = value;
        }),
    __setIndex: (object, key, value) =>
        op('setIndex', { object: native(object), key: native(key), value: native(value) }, () => {
            object[key] = value;
        }),
    __defineIndex: (object, key, value) =>
        op('defineIndex', { object: native(object), key: native(key), value: native(value) }, () => {
            Object.defineProperty(object, key, { value, writable: true, enumerable: true, configurable: true });
        }),
    __call: (fn, receiver, ...args) => {
        const e = {
            op: 'call',
            args: { function: native(fn), receiver: native(receiver), args: args.map(native) },
            invokes: [],
        };
        calls.push(e);
        activeCalls.push(e);
        try {
            if (g.throwCall === tags.get(fn)) throw g.error;
            const result = Reflect.apply(fn, receiver, args);
            e.result = native(result);
            if (Array.isArray(result)) {
                tag(result, 'array');
                e.result = native(result);
                e.elements = Array.from(result, native);
                e.prototype = native(Object.getPrototypeOf(result));
            }
            return result;
        } catch (error) {
            e.throw = native(error);
            throw error;
        } finally {
            assert.equal(activeCalls.pop(), e);
        }
    },
    __newObject: () => {
        const value = op('object', {}, () => tag({}, 'literal'));
        calls.at(-1).prototype = native(Object.getPrototypeOf(value));
        return value;
    },
    __newArray: () => {
        const value = op('array', {}, () => tag([], 'array'));
        calls.at(-1).prototype = native(Object.getPrototypeOf(value));
        return value;
    },
    __arrayDefine: (object, index, value) =>
        op('arrayDefine', { object: native(object), index, value: native(value) }, () => {
            Object.defineProperty(object, index, { value, writable: true, enumerable: true, configurable: true });
        }),
    __define: (object, name, value) =>
        op('define', { object: native(object), name, value: native(value) }, () => {
            Object.defineProperty(object, name, { value, writable: true, enumerable: true, configurable: true });
        }),
    __spread: (object, value) => {
        const e = { op: 'spread', args: { object: native(object), source: native(value), members: [] } };
        calls.push(e);
        try {
            if (value != null)
                for (const key of Reflect.ownKeys(Object(value))) {
                    const d = Object.getOwnPropertyDescriptor(Object(value), key);
                    if (d?.enumerable) {
                        const member = value[key];
                        e.args.members.push([key, native(member)]);
                        Object.defineProperty(object, key, {
                            value: member,
                            writable: true,
                            enumerable: true,
                            configurable: true,
                        });
                    }
                }
            e.result = native(undefined);
        } catch (error) {
            e.throw = native(error);
            throw error;
        }
    },
    __callback: (kind, captures, fn, isAsync) =>
        op('restCallback', { kind, captures: captures.map(native), isAsync }, () => {
            const wrapped = function (...args) {
                const current = activeCalls.at(-1);
                const event = {
                    op: 'invoke',
                    args: { function: native(wrapped), receiver: native(this), args: args.map(native) },
                };
                calls.push(event);
                if (current) current.invokes.push(event);
                if (!isAsync) {
                    try {
                        const result = Reflect.apply(fn, this, args);
                        event.result = native(result);
                        return result;
                    } catch (error) {
                        event.throw = native(error);
                        throw error;
                    }
                }
                const promise = Reflect.apply(fn, this, args);
                const output = Promise.resolve(promise).then(
                    (value) => {
                        op('settle', { kind, success: true, value: native(value) }, () => undefined);
                        event.completed = native(value);
                        return value;
                    },
                    (error) => {
                        op('settle', { kind, success: false, value: native(error) }, () => undefined);
                        event.rejected = native(error);
                        throw error;
                    },
                );
                tag(output, 'promise');
                event.result = native(output);
                return output;
            };
            return tag(wrapped, 'restCallback:' + kind);
        }),
    __arrayBinding: (value, mask) => {
        const event = { op: 'arrayBinding', args: { value: native(value), mask } };
        calls.push(event);
        try {
            const pattern = mask.map((yes, i) => (yes ? 'v' + i : '')).join(',');
            const read = new Function(
                'value',
                `const [${pattern}]=value;return [${mask.map((yes, i) => (yes ? 'v' + i : 'undefined')).join(',')}];`,
            );
            const result = read(value);
            event.result = result.map(native);
            return result;
        } catch (error) {
            event.throw = native(error);
            throw error;
        }
    },
    __argumentSpread: (value) => Array.from(bind.__iterate(value)),
    __regex: (pattern) =>
        op('regex', { pattern }, () =>
            tag(
                new RegExp(pattern.slice(1, pattern.lastIndexOf('/')), pattern.slice(pattern.lastIndexOf('/') + 1)),
                'regex',
            ),
        ),
    __pair: (value) => {
        const e = { op: 'pair', args: { value: native(value) } };
        calls.push(e);
        try {
            const [a, b] = value;
            e.result = [native(a), native(b)];
            return [a, b];
        } catch (error) {
            e.throw = native(error);
            throw error;
        }
    },
    __template: (value) => op('template', { value: native(value) }, () => `${value}`),
    __operator: (operator, left, right) =>
        op('operator', { operator, left: native(left), right: native(right) }, () => {
            switch (operator) {
                case 'add':
                    return left + right;
                case 'subtract':
                    return left - right;
                case 'multiply':
                    return left * right;
                case 'greater':
                    return left > right;
                case 'less':
                    return left < right;
                case 'lessEqual':
                    return left <= right;
                case 'greaterEqual':
                    return left >= right;
                case 'divide':
                    return left / right;
                default:
                    throw new Error(operator);
            }
        }),
    __iterate: (value) =>
        op('iterator', { value: native(value) }, () => {
            const real = value[Symbol.iterator]();
            if (real === null || (typeof real !== 'object' && typeof real !== 'function'))
                throw new TypeError('iterator object');
            const nextMethod = real.next;
            let count = 0;
            const iterator = tag(
                {
                    [Symbol.iterator]() {
                        return this;
                    },
                    next() {
                        const e = { op: 'next', args: { iterator: native(iterator) } };
                        calls.push(e);
                        try {
                            if (++count === g.throwNext) throw g.error;
                            const step = Reflect.apply(nextMethod, real, []);
                            if (step === null || (typeof step !== 'object' && typeof step !== 'function'))
                                throw new TypeError('iterator result object');
                            const done = !!step.done,
                                value = done ? undefined : step.value;
                            e.result = done ? ['done'] : native(value);
                            return { done, value };
                        } catch (error) {
                            e.throw = native(error);
                            throw error;
                        }
                    },
                    return() {
                        return op('close', { iterator: native(iterator) }, () => {
                            if (g.throwClose) throw g.closeError;
                            const close = real.return;
                            if (close == null) return {};
                            const result = Reflect.apply(close, real, []);
                            if (result === null || (typeof result !== 'object' && typeof result !== 'function'))
                                throw new TypeError('iterator return object');
                            return result;
                        });
                    },
                },
                'iterator',
            );
            return iterator;
        }),
};
const globalNames = metadata.globals;
function transformer(context) {
    const f = ts.factory,
        call = (name, args) => f.createCallExpression(f.createIdentifier(name), undefined, args),
        variable = (name, value) =>
            f.createVariableStatement(
                undefined,
                f.createVariableDeclarationList(
                    [f.createVariableDeclaration(name, undefined, undefined, value)],
                    ts.NodeFlags.Const,
                ),
            );
    const bound = (n, name) => {
        for (let p = n.parent; p; p = p.parent)
            if (ts.isArrowFunction(p) || ts.isFunctionExpression(p)) {
                if (p.parameters.flatMap((p) => metadata.bindingNames(p.name)).includes(name)) return true;
            }
        return false;
    };
    const bindObject = (pattern, value) => {
        const object = f.createIdentifier('__binding' + pattern.pos);
        const declarations = [f.createVariableDeclaration(object, undefined, undefined, value)];
        for (const e of pattern.elements) {
            const property = e.propertyName?.text ?? e.name.text;
            let get = call('__get', [object, f.createStringLiteral(property)]);
            if (e.initializer) {
                const temp = f.createIdentifier('__bound' + e.pos);
                declarations.push(f.createVariableDeclaration(temp, undefined, undefined, get));
                get = f.createConditionalExpression(
                    f.createBinaryExpression(
                        temp,
                        f.createToken(ts.SyntaxKind.EqualsEqualsEqualsToken),
                        f.createIdentifier('undefined'),
                    ),
                    f.createToken(ts.SyntaxKind.QuestionToken),
                    ts.visitNode(e.initializer, visit),
                    f.createToken(ts.SyntaxKind.ColonToken),
                    temp,
                );
            }
            declarations.push(f.createVariableDeclaration(e.name, undefined, undefined, get));
        }
        return declarations;
    };
    const visit = (n) => {
        if (ts.isTypeNode(n)) return n;
        if (ts.isArrowFunction(n)) {
            let result = ts.visitEachChild(n, visit, context);
            const parameters = [],
                prefix = [];
            for (let i = 0; i < n.parameters.length; i++) {
                const original = n.parameters[i];
                if (ts.isObjectBindingPattern(original.name)) {
                    const raw = f.createIdentifier('__parameter' + i);
                    parameters.push(
                        f.updateParameterDeclaration(
                            result.parameters[i],
                            result.parameters[i].modifiers,
                            undefined,
                            raw,
                            undefined,
                            undefined,
                            result.parameters[i].initializer,
                        ),
                    );
                    prefix.push(
                        f.createVariableStatement(
                            undefined,
                            f.createVariableDeclarationList(bindObject(original.name, raw), ts.NodeFlags.Const),
                        ),
                    );
                } else parameters.push(result.parameters[i]);
            }
            if (prefix.length)
                result = f.updateArrowFunction(
                    result,
                    result.modifiers,
                    result.typeParameters,
                    parameters,
                    result.type,
                    result.equalsGreaterThanToken,
                    f.createBlock(
                        [
                            ...prefix,
                            ...(ts.isBlock(result.body)
                                ? result.body.statements
                                : [f.createReturnStatement(result.body)]),
                        ],
                        true,
                    ),
                );
            if (metadata.methods.some((m) => m.fn === n)) return result;
            const c = metadata.callbacks.find((c) => c.node === n);
            assert(c, 'original callback identity ' + n.getText());
            return call('__callback', [
                f.createStringLiteral(c.kind),
                f.createArrayLiteralExpression(
                    c.captures.map((v) => (v === 'this_value' ? f.createThis() : f.createIdentifier(v))),
                ),
                result,
                c.async ? f.createTrue() : f.createFalse(),
            ]);
        }
        if (
            ts.isVariableStatement(n) &&
            n.declarationList.declarations.some((d) => ts.isObjectBindingPattern(d.name))
        ) {
            const declarations = [];
            for (const d of n.declarationList.declarations) {
                if (ts.isObjectBindingPattern(d.name))
                    declarations.push(...bindObject(d.name, ts.visitNode(d.initializer, visit)));
                else declarations.push(ts.visitNode(d, visit));
            }
            return f.updateVariableStatement(
                n,
                n.modifiers,
                f.createVariableDeclarationList(declarations, n.declarationList.flags),
            );
        }
        if (ts.isVariableDeclaration(n) && ts.isArrayBindingPattern(n.name)) {
            const mask = n.name.elements.map((e) => !ts.isOmittedExpression(e));
            return f.updateVariableDeclaration(
                n,
                n.name,
                n.exclamationToken,
                n.type,
                call('__arrayBinding', [
                    ts.visitNode(n.initializer, visit),
                    f.createArrayLiteralExpression(mask.map((v) => (v ? f.createTrue() : f.createFalse()))),
                ]),
            );
        }
        if (ts.isAwaitExpression(n))
            return f.updateAwaitExpression(n, call('__await', [ts.visitNode(n.expression, visit)]));
        if (ts.isPrefixUnaryExpression(n) && [ts.SyntaxKind.PlusToken, ts.SyntaxKind.MinusToken].includes(n.operator))
            return call('__unary', [
                f.createStringLiteral(n.operator === ts.SyntaxKind.PlusToken ? 'plus' : 'minus'),
                ts.visitNode(n.operand, visit),
            ]);
        if (ts.isForOfStatement(n)) {
            const expression = call('__iterate', [ts.visitNode(n.expression, visit)]),
                body = ts.visitNode(n.statement, visit);
            if (
                ts.isVariableDeclarationList(n.initializer) &&
                n.initializer.declarations.length === 1 &&
                ts.isArrayBindingPattern(n.initializer.declarations[0].name)
            ) {
                const declaration = n.initializer.declarations[0];
                assert.equal(declaration.name.elements.length, 2, 'tuple binding floor');
                const raw = f.createIdentifier('__tupleValue'),
                    initializer = f.createVariableDeclarationList(
                        [f.createVariableDeclaration(raw)],
                        ts.NodeFlags.Const,
                    );
                const binding = variable(declaration.name, call('__pair', [raw]));
                return f.updateForOfStatement(
                    n,
                    n.awaitModifier,
                    initializer,
                    expression,
                    f.createBlock([binding, ...(ts.isBlock(body) ? body.statements : [body])], true),
                );
            }
            return f.updateForOfStatement(n, n.awaitModifier, n.initializer, expression, body);
        }
        if (ts.isRegularExpressionLiteral(n)) return call('__regex', [f.createStringLiteral(n.text)]);
        if (ts.isTemplateExpression(n))
            return f.updateTemplateExpression(
                n,
                n.head,
                n.templateSpans.map((s) =>
                    f.updateTemplateSpan(s, call('__template', [ts.visitNode(s.expression, visit)]), s.literal),
                ),
            );
        if (ts.isBinaryExpression(n)) {
            const kind = n.operatorToken.kind;
            if (kind === ts.SyntaxKind.InstanceOfKeyword)
                return call('__instanceof', [ts.visitNode(n.left, visit), ts.visitNode(n.right, visit)]);
            if (kind === ts.SyntaxKind.EqualsToken && ts.isPropertyAccessExpression(n.left))
                return call('__assign', [
                    ts.visitNode(n.left.expression, visit),
                    f.createStringLiteral(n.left.name.text),
                    ts.visitNode(n.right, visit),
                ]);
            if (kind === ts.SyntaxKind.EqualsToken && ts.isElementAccessExpression(n.left))
                return call('__setIndex', [
                    ts.visitNode(n.left.expression, visit),
                    ts.visitNode(n.left.argumentExpression, visit),
                    ts.visitNode(n.right, visit),
                ]);
            const ops = new Map([
                [ts.SyntaxKind.PlusToken, 'add'],
                [ts.SyntaxKind.MinusToken, 'subtract'],
                [ts.SyntaxKind.AsteriskToken, 'multiply'],
                [ts.SyntaxKind.GreaterThanToken, 'greater'],
                [ts.SyntaxKind.PlusEqualsToken, 'add'],
                [ts.SyntaxKind.LessThanToken, 'less'],
                [ts.SyntaxKind.LessThanEqualsToken, 'lessEqual'],
                [ts.SyntaxKind.GreaterThanEqualsToken, 'greaterEqual'],
                [ts.SyntaxKind.SlashToken, 'divide'],
            ]);
            if (ops.has(kind)) {
                const op = call('__operator', [
                    f.createStringLiteral(ops.get(kind)),
                    ts.visitNode(n.left, visit),
                    ts.visitNode(n.right, visit),
                ]);
                return kind === ts.SyntaxKind.PlusEqualsToken
                    ? f.createBinaryExpression(n.left, f.createToken(ts.SyntaxKind.EqualsToken), op)
                    : op;
            }
        }
        if (ts.isNewExpression(n))
            return call('__construct', [
                ts.visitNode(n.expression, visit),
                ...(n.arguments ?? []).map((a) => ts.visitNode(a, visit)),
            ]);
        if (ts.isCallExpression(n) && !ts.isPropertyAccessExpression(n.expression))
            return call('__call', [
                ts.visitNode(n.expression, visit),
                f.createIdentifier('undefined'),
                ...n.arguments.map((a) =>
                    ts.isSpreadElement(a)
                        ? f.createSpreadElement(call('__argumentSpread', [ts.visitNode(a.expression, visit)]))
                        : ts.visitNode(a, visit),
                ),
            ]);
        if (ts.isCallExpression(n) && ts.isPropertyAccessExpression(n.expression)) {
            const recv = f.createIdentifier('__receiver'),
                fn = f.createIdentifier('__function');
            const statements = [variable(recv, ts.visitNode(n.expression.expression, visit))];
            if (n.expression.questionDotToken)
                statements.push(
                    f.createIfStatement(
                        f.createBinaryExpression(recv, f.createToken(ts.SyntaxKind.EqualsEqualsToken), f.createNull()),
                        f.createReturnStatement(f.createIdentifier('undefined')),
                    ),
                );
            const args = n.arguments.map((a) =>
                ts.isSpreadElement(a)
                    ? f.createSpreadElement(call('__argumentSpread', [ts.visitNode(a.expression, visit)]))
                    : ts.visitNode(a, visit),
            );
            statements.push(variable(fn, call('__get', [recv, f.createStringLiteral(n.expression.name.text)])));
            if (n.questionDotToken)
                statements.push(
                    f.createIfStatement(
                        f.createBinaryExpression(fn, f.createToken(ts.SyntaxKind.EqualsEqualsToken), f.createNull()),
                        f.createReturnStatement(f.createIdentifier('undefined')),
                    ),
                );
            statements.push(f.createReturnStatement(call('__call', [fn, recv, ...args])));
            return f.createCallExpression(
                f.createParenthesizedExpression(
                    f.createArrowFunction(
                        undefined,
                        undefined,
                        [],
                        undefined,
                        f.createToken(ts.SyntaxKind.EqualsGreaterThanToken),
                        f.createBlock(statements, true),
                    ),
                ),
                undefined,
                [],
            );
        }
        if (ts.isPropertyAccessExpression(n) || ts.isElementAccessExpression(n)) {
            const property = ts.isPropertyAccessExpression(n),
                key = property ? f.createStringLiteral(n.name.text) : ts.visitNode(n.argumentExpression, visit);
            if (ts.isPropertyAccessExpression(n.expression) && n.expression.questionDotToken) {
                const receiver = f.createIdentifier('__optionalReceiver');
                const inner = call('__get', [receiver, f.createStringLiteral(n.expression.name.text)]);
                const tail = call(
                    property
                        ? n.questionDotToken
                            ? '__optionalGet'
                            : '__get'
                        : n.questionDotToken
                          ? '__optionalIndex'
                          : '__index',
                    [inner, key],
                );
                const ret = f.createConditionalExpression(
                    f.createBinaryExpression(receiver, f.createToken(ts.SyntaxKind.EqualsEqualsToken), f.createNull()),
                    f.createToken(ts.SyntaxKind.QuestionToken),
                    f.createIdentifier('undefined'),
                    f.createToken(ts.SyntaxKind.ColonToken),
                    tail,
                );
                return f.createCallExpression(
                    f.createParenthesizedExpression(
                        f.createArrowFunction(
                            undefined,
                            undefined,
                            [],
                            undefined,
                            f.createToken(ts.SyntaxKind.EqualsGreaterThanToken),
                            f.createBlock(
                                [
                                    variable(receiver, ts.visitNode(n.expression.expression, visit)),
                                    f.createReturnStatement(ret),
                                ],
                                true,
                            ),
                        ),
                    ),
                    undefined,
                    [],
                );
            }
            const get = call(property ? '__get' : '__index', [ts.visitNode(n.expression, visit), key]);
            if (n.questionDotToken) {
                const receiver = f.createIdentifier('__optionalReceiver');
                const ret = f.createConditionalExpression(
                    f.createBinaryExpression(receiver, f.createToken(ts.SyntaxKind.EqualsEqualsToken), f.createNull()),
                    f.createToken(ts.SyntaxKind.QuestionToken),
                    f.createIdentifier('undefined'),
                    f.createToken(ts.SyntaxKind.ColonToken),
                    call(property ? '__get' : '__index', [receiver, key]),
                );
                return f.createCallExpression(
                    f.createParenthesizedExpression(
                        f.createArrowFunction(
                            undefined,
                            undefined,
                            [],
                            undefined,
                            f.createToken(ts.SyntaxKind.EqualsGreaterThanToken),
                            f.createBlock(
                                [variable(receiver, ts.visitNode(n.expression, visit)), f.createReturnStatement(ret)],
                                true,
                            ),
                        ),
                    ),
                    undefined,
                    [],
                );
            }
            return get;
        }
        if (ts.isIdentifier(n) && globalNames.has(n.text) && !bound(n, n.text))
            return call('__global', [f.createStringLiteral(n.text)]);
        if (ts.isArrayLiteralExpression(n)) {
            const array = f.createIdentifier('__array');
            const statements = [variable(array, call('__newArray', []))];
            n.elements.forEach((v, i) =>
                statements.push(
                    f.createExpressionStatement(
                        ts.isSpreadElement(v)
                            ? call('__appendSpread', [array, ts.visitNode(v.expression, visit)])
                            : call('__arrayDefine', [array, f.createNumericLiteral(i), ts.visitNode(v, visit)]),
                    ),
                ),
            );
            return f.createCallExpression(
                f.createParenthesizedExpression(
                    f.createArrowFunction(
                        undefined,
                        undefined,
                        [],
                        undefined,
                        f.createToken(ts.SyntaxKind.EqualsGreaterThanToken),
                        f.createBlock([...statements, f.createReturnStatement(array)], true),
                    ),
                ),
                undefined,
                [],
            );
        }
        if (ts.isObjectLiteralExpression(n)) {
            const object = f.createIdentifier('__literal');
            const statements = [variable(object, call('__newObject', []))];
            for (const p of n.properties) {
                if (ts.isSpreadAssignment(p)) {
                    statements.push(
                        f.createExpressionStatement(call('__spread', [object, ts.visitNode(p.expression, visit)])),
                    );
                    continue;
                }
                assert(ts.isShorthandPropertyAssignment(p) || ts.isPropertyAssignment(p));
                const value = ts.isShorthandPropertyAssignment(p)
                    ? ts.visitNode(p.name, visit)
                    : ts.visitNode(p.initializer, visit);
                const computed = ts.isComputedPropertyName(p.name);
                const key = computed
                    ? ts.visitNode(p.name.expression, visit)
                    : f.createStringLiteral(
                          ts.isStringLiteral(p.name) || ts.isNumericLiteral(p.name) ? p.name.text : p.name.getText(),
                      );
                if (computed) {
                    const k = f.createIdentifier('__computedKey' + statements.length);
                    statements.push(
                        variable(k, key),
                        f.createExpressionStatement(call('__defineIndex', [object, k, value])),
                    );
                } else statements.push(f.createExpressionStatement(call('__define', [object, key, value])));
            }
            return f.createCallExpression(
                f.createParenthesizedExpression(
                    f.createArrowFunction(
                        undefined,
                        undefined,
                        [],
                        undefined,
                        f.createToken(ts.SyntaxKind.EqualsGreaterThanToken),
                        f.createBlock([...statements, f.createReturnStatement(object)], true),
                    ),
                ),
                undefined,
                [],
            );
        }
        return ts.visitEachChild(n, visit, context);
    };
    return (s) => ts.visitNode(s, visit);
}
function compile(source) {
    const context = { exports: {} };
    vm.runInNewContext(
        ts.transpileModule(source, {
            compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
        }).outputText,
        context,
    );
    return context.exports;
}
const directory = path.join(root, 'packages/safe-cex/src/exchanges/hyperliquid');
const constants = compile(fs.readFileSync(path.join(directory, 'hyperliquid.constants.ts'), 'utf8'));
const typesFile = path.join(directory, 'hyperliquid.types.ts'),
    typesAst = ts.createSourceFile(typesFile, fs.readFileSync(typesFile, 'utf8'), ts.ScriptTarget.Latest, true),
    endpoints = compile(
        typesAst.statements
            .find(
                (n) =>
                    ts.isVariableStatement(n) &&
                    n.declarationList.declarations.some((d) => d.name.getText(typesAst) === 'ENDPOINTS'),
            )
            .getText(typesAst),
    ).ENDPOINTS;
const infoTypes = compile(
    typesAst.statements
        .find(
            (n) =>
                ts.isVariableStatement(n) &&
                n.declarationList.declarations.some((d) => d.name.getText(typesAst) === 'INFO_TYPES'),
        )
        .getText(typesAst),
).INFO_TYPES;
let activeRoot;
const printer = ts.createPrinter(),
    classes = {};
for (const owner of ['core']) {
    const { ast, cls } = metadata.parsed[owner];
    const code = metadata.methods
        .filter((m) => m.owner === owner)
        .map((m) => {
            activeRoot = m.name;
            const transformed = ts.transform(m.member, [transformer]);
            const printed = printer.printNode(ts.EmitHint.Unspecified, transformed.transformed[0], ast);
            transformed.dispose();
            return printed;
        })
        .join('\n');
    const constant = cls.members.find((m) => m.name?.getText(ast) === 'TESTNET_HIP3_WHITELIST').getText(ast);
    const className = owner === 'private' ? 'HyperliquidPrivateWebSocket' : 'HyperliquidExchange';
    const context = { ...bind, exports: {} };
    vm.runInNewContext(
        ts.transpileModule(`class ${className}{${constant}\n${code}}exports.Class=${className};`, {
            compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
        }).outputText,
        context,
    );
    classes[owner] = context.exports.Class;
}
const fieldNames = [],
    cases = [];
const effectNames = ['post', 'log', 'emitAndLogError', 'resolveCoinToSymbol', 'fetchIncomeHistory'];
const traceIn = (frame, fn, ...args) => {
    activeCalls.push(frame);
    try {
        return fn(...args);
    } finally {
        assert.equal(activeCalls.pop(), frame);
    }
};
const marketRow = (extra = {}) => ({ name: 'BTC', szDecimals: 3, maxLeverage: 17.5, ...extra });
const tickerRow = (extra = {}) => ({
    midPx: '91.25',
    markPx: '17.5',
    prevDayPx: '12.25',
    oraclePx: '93.5',
    funding: '0.25',
    dayNtlVlm: '501.25',
    openInterest: '4.75',
    ...extra,
});
const currentTime = 1_700_000_000_150;
const fill = (extra = {}) => ({
    tid: 150,
    oid: 138,
    coin: 'BTC',
    time: currentTime,
    px: '91.25tail',
    sz: '1.25',
    side: 'B',
    fee: '-0.25',
    builderFee: '-0.125',
    closedPnl: '3.5',
    crossed: true,
    ...extra,
});
const history = (extra = {}) => ({
    oid: 150,
    coin: 'BTC',
    time: currentTime,
    createdTime: currentTime,
    symbol: 'BTC-PERP',
    ...extra,
});
async function add(name, label, args = [], config = {}) {
    ids = new WeakMap();
    tags = new WeakMap();
    nextId = 1;
    calls = [];
    activeCalls = [];
    tag(Object.prototype, 'defaultPrototype');
    tag(Array.prototype, 'arrayPrototype');
    tag(Map.prototype, 'mapPrototype');
    tag(Set.prototype, 'setPrototype');
    const e = tag(new classes.core(), 'exchange'),
        selected = e[name],
        effects = [],
        error = tag(
            Object.assign(new Error('native rejection'), { response: { data: { message: 'response-150' } } }),
            'error',
        );
    g = { error, ...config, state: () => [], afterAwait() {} };
    e.options = tag(
        { key: 'api-wallet', applicationId: 'main-wallet', vault: 'vault-wallet', testnet: false, ...config.options },
        'options',
    );
    e.isPublicDataOnly = config.publicOnly ?? false;
    const nativeMarket = tag({ id: 'BTC', symbol: 'BTC-PERP', info: { name: 'BTC' } }, 'market');
    e.store = tag({ marketsBySymbol: { 'BTC-PERP': nativeMarket }, markets: [nativeMarket] }, 'store');
    if (config.missingMarket) e.store.marketsBySymbol = {};
    if (config.noCandleMarket) e.store.markets = [];
    if (config.noFind) e.store.markets = { find: undefined };
    function virtual(target, n, fn) {
        target[n] = tag(function (...args) {
            effects.push({ name: n, receiver: native(this), args: args.map(tree) });
            return fn ? fn(...args) : undefined;
        }, n);
    }
    virtual(e, 'log');
    virtual(e, 'emitAndLogError');
    virtual(e, 'resolveCoinToSymbol', (coin) =>
        coin === 'BTC' ? 'BTC-PERP' : coin === 'ETH' ? 'ETH-PERP' : undefined,
    );
    virtual(e, 'fetchIncomeHistory', () =>
        config.rejectRange
            ? Promise.reject(error)
            : Promise.resolve(tag([{ symbol: 'BTC-PERP', amount: 91.25 }], 'fixtureRecords')),
    );
    let postCount = 0;
    for (const which of ['xhr', 'infoXhr']) {
        e[which] = tag({}, which);
        virtual(e[which], 'post', (_endpoint, body) => {
            postCount++;
            if (config.rejectPost === true || config.rejectPost === postCount) return Promise.reject(error);
            let data;
            if (Object.hasOwn(config, 'data')) data = config.data;
            else if (name === 'fetchHip3Dexs')
                data = [null, { name: 'xyz', dexPosition: 999 }, false, { name: 'VOLMEX' }, { name: 'unknown' }];
            else if (name === 'fetchOHLCV')
                data = [{ t: 150000, o: '17.5', h: '93.5', l: '12.25', c: '91.25', v: '4.75' }];
            else if (name === 'fetchHistoricalOrders')
                data = [
                    history(),
                    history({ oid: 151, coin: 'ETH', symbol: 'ETH-PERP', createdTime: currentTime - 100 }),
                ];
            else if (name === 'fetchIncomeHistory' || name === 'deriveClosedPositions')
                data =
                    body.type === infoTypes.USER_FUNDING
                        ? [{ hash: 'funding-138', time: currentTime, delta: { coin: 'BTC', usdc: '3.5' } }]
                        : [fill()];
            else
                data = config.responses?.[postCount - 1] ?? [
                    fill(),
                    fill({ tid: 151, fee: '-0.5' }),
                    fill({ side: 'A', sz: '4.75' }),
                ];
            if (config.snapshotCap && name === 'fetchHistoricalOrders') data.length = 2000;
            return Promise.resolve(tag({ data }, 'response'));
        });
    }
    const primitive = (name, fn) => (globals[name] = tag(fn, name));
    Object.assign(globals, {
        Object: tag(Object, 'Object'),
        Number: tag(Number, 'Number'),
        String: tag(String, 'String'),
        Boolean: tag(Boolean, 'Boolean'),
        Array: tag(Array, 'Array'),
        Math: tag(Math, 'Math'),
        Map: tag(Map, 'Map'),
        Set: tag(Set, 'Set'),
        Error: tag(Error, 'Error'),
        JSON: tag(JSON, 'JSON'),
        parseFloat: tag(parseFloat, 'parseFloat'),
        parseInt: tag(parseInt, 'parseInt'),
        ENDPOINTS: tag(endpoints, 'ENDPOINTS'),
        INFO_TYPES: tag(infoTypes, 'INFO_TYPES'),
        LogSeverity: tag({ Error: 'error', Info: 'info', Debug: 'debug', Warning: 'warning' }, 'LogSeverity'),
        OrderSide: tag({ Buy: 'buy', Sell: 'sell' }, 'OrderSide'),
        virtualClock: tag({ getCurrentTimeMs: () => currentTime }, 'virtualClock'),
        HyperliquidExchange: tag(classes.core, 'HyperliquidExchange'),
    });
    primitive('v', (object, name) => object?.[name]);
    primitive('safeString', (object, name) => (object?.[name] == null ? undefined : String(object[name])));
    primitive('calculatePricePrecision', () => 0.01);
    primitive('numberToString', String);
    primitive('parsePrecision', (s) => (s === undefined ? undefined : Math.pow(10, -Number(s))));
    primitive('parseFiniteNumber', (v) => {
        const n = parseFloat(v);
        return Number.isFinite(n) ? n : 0;
    });
    primitive('parseHyperliquidVolumeFields', () => tag({ volume: 4.75, quoteVolume: 501.25 }, 'volume'));
    primitive('calculatePriceChangePercentage', (last, prev) => ((last - prev) / prev) * 100);
    primitive('parseHyperliquidOpenInterestNotional', (ticker, price) => Number(ticker.openInterest) * price);
    primitive('symbolToCoin', (s) => s.replace(/-PERP$/i, '').toLowerCase());
    primitive('fetchHip3MetaAndAssetCtxsWithRetry', (details) => {
        const frame = activeCalls.at(-1);
        traceIn(frame, details.log, 'metadata retry');
        return config.rejectRetry
            ? Promise.reject(error)
            : Promise.resolve(
                  Object.hasOwn(config, 'parsed')
                      ? config.parsed
                      : tag(
                            [
                                { universe: [marketRow(), marketRow({ name: 'xyz:ETH' })] },
                                [tickerRow(), tickerRow({ midPx: '17.5' })],
                            ],
                            'tuple',
                        ),
              );
    });
    primitive('dayjs', (value) => {
        const date = tag({ time: value ?? currentTime }, 'date');
        date.subtract = function (amount, unit) {
            const factor = unit === 'h' ? 3600000 : unit === 'd' ? 86400000 : 60000;
            return globals.dayjs(date.time - amount * factor);
        };
        date.valueOf = function () {
            return date.time;
        };
        return date;
    });
    primitive('paginateHyperliquidByTime', async (post, type, start, end, key, onCap) => {
        const frame = activeCalls.at(-1);
        const body = tag({ startTime: start, endTime: end, ...config.bodyOverride }, 'pageBody');
        const rows = await (config.bodyUndefined ? traceIn(frame, post, type) : traceIn(frame, post, type, body));
        const seen = new Set(),
            result = [];
        for (const row of rows) {
            const id = traceIn(frame, key, row);
            if (!seen.has(id)) {
                seen.add(id);
                result.push(row);
            }
        }
        if (config.capHit && onCap) traceIn(frame, onCap, type, 1);
        return tag(result, 'pageRows');
    });
    primitive('mapHyperliquidFillToIncomeRecords', (row, resolve) => {
        const frame = activeCalls.at(-1),
            symbol = traceIn(frame, resolve, row.coin);
        return tag(
            [
                { symbol, timestamp: row.time, id: row.tid, amount: 91.25 },
                { symbol, timestamp: row.time - 1, id: 'fee', amount: -0.25 },
            ].map((o) => tag(o, 'mappedIncome')),
            'mappedIncomeRows',
        );
    });
    primitive('mapHyperliquidFundingToIncomeRecord', (row, resolve) => {
        const frame = activeCalls.at(-1);
        return tag(
            {
                symbol: traceIn(frame, resolve, (row.delta ?? row).coin),
                timestamp: row.time,
                id: row.hash,
                amount: 3.5,
            },
            'mappedFunding',
        );
    });
    primitive('collapseHyperliquidOrderHistory', (rows) => tag(rows.filter(Boolean), 'collapsedRows'));
    primitive('mapHyperliquidHistoricalOrder', (row, resolve) => {
        const frame = activeCalls.at(-1);
        return config.nullOrders ? null : tag({ ...row, symbol: traceIn(frame, resolve, row.coin) }, 'mappedOrder');
    });
    primitive('deriveHyperliquidClosedPositions', (rows, resolve, accountName) => {
        const frame = activeCalls.at(-1);
        return tag(
            rows.map((row) =>
                tag(
                    { symbol: traceIn(frame, resolve, row.coin), accountName, contracts: 4.75, id: row.tid },
                    'derivedClose',
                ),
            ),
            'derivedRows',
        );
    });
    args = args.map((value, index) =>
        value && typeof value === 'object' && !tags.has(value) ? tag(value, 'argument' + index) : value,
    );
    const input = native(e),
        inputArgs = args.map(native),
        seedArrays = args
            .filter(Array.isArray)
            .map((value) => ({ value: native(value), elements: Array.from(value, native) }));
    let result,
        started = false,
        prefixOps;
    try {
        const value = Reflect.apply(selected, e, args);
        started = true;
        prefixOps = calls.length;
        const resolved = metadata.methods.find((m) => m.name === name).async ? await value : value;
        if (metadata.methods.find((m) => m.name === name).async)
            op('settle', { kind: 'root.' + name, success: true, value: native(resolved) }, () => undefined);
        result = { value: tree(resolved) };
    } catch (error) {
        prefixOps ??= calls.length;
        if (metadata.methods.find((m) => m.name === name).async)
            op('settle', { kind: 'root.' + name, success: false, value: native(error) }, () => undefined);
        result = { throw: native(error) };
    }
    cases.push({
        name,
        label,
        input,
        args: inputArgs,
        calls,
        initial: [],
        final: [],
        seedArrays,
        effects,
        result,
        started,
        prefixOps,
        expectedAwaits: calls.filter((c) => c.op === 'await').length,
        postCount,
    });
}
(async () => {
    const meta = (universe = [marketRow(), marketRow({ name: 'ETH' })]) => ({ universe });
    const assets = () => [tickerRow(), tickerRow({ midPx: '17.5', markPx: '91.25' })];
    for (const [label, m, a, fees, c] of [
        ['fields', meta(), assets(), { takerFee: 0.0025, makerFee: 0.00125 }, {}],
        ['null fees', meta(), assets(), null, {}],
        ['empty', meta([]), [], null, {}],
        ['deprecated', meta(), [tickerRow({ midPx: 0 }), undefined], null, {}],
        ['info overrides', meta([marketRow({ hyperliquidId: 91.25 })]), assets(), null, {}],
        ['orphan market', meta(), assets(), null, { throwCall: 'parseFiniteNumber' }],
        ['outside error', meta(), assets(), null, { throwCall: 'v' }],
        ['row getter error', meta(), assets(), null, { throwGet: 'MAX_SAFE_INTEGER' }],
    ])
        await add('fetchMarketsHelper', label, [m, a, fees], c);
    for (const [label, c] of [
        ['production', {}],
        ['testnet', { options: { testnet: true } }],
        ['empty', { data: [] }],
        ['null data', { data: null }],
        ['reject', { rejectPost: true }],
        ['map error', { throwCall: 'map' }],
        ['whitelist name error', { options: { testnet: true }, throwGet: 'name' }],
        ['logger error', { rejectPost: true, throwCall: 'log' }],
    ])
        await add('fetchHip3Dexs', label, [], c);
    for (const [label, c] of [
        ['markets', {}],
        ['no response', { parsed: null }],
        ['empty', { parsed: [meta([]), []] }],
        ['delisted', { parsed: [meta([marketRow({ isDelisted: true }), marketRow()]), assets()] }],
        [
            'prefix and info',
            {
                parsed: [
                    meta([marketRow({ name: 'xyz:ETH:EXTRA', hyperliquidId: 91.25, hip3Dex: 'override' })]),
                    assets(),
                ],
            },
        ],
        ['retry rejection outside catch', { rejectRetry: true }],
        ['bad tuple', { parsed: 17.5 }],
        ['orphan market', { throwCall: 'parseFiniteNumber' }],
    ])
        await add('fetchHip3Markets', label, ['xyz', 2, { takerFee: 0.0025, makerFee: 0.00125 }], c);
    const candle = (extra = {}) => ({ symbol: 'BTC-PERP', interval: '1h', limit: 2, ...extra });
    for (const [label, opts, c] of [
        ['resolved coin', candle(), {}],
        ['regular fallback', candle({ symbol: 'ETH-PERP' }), { noCandleMarket: true }],
        ['HIP3 fallback', candle({ symbol: 'XYZ|MixedCase-PERP' }), { noCandleMarket: true }],
        ['optional callable', candle(), { noFind: true }],
        ['from branch', candle({ limit: 0, from: currentTime - 1000, to: currentTime }), {}],
        [
            'wrapped data',
            candle(),
            { data: { data: [{ t: 150000, o: '17.5', h: '93.5', l: '12.25', c: '91.25', v: '4.75' }] } },
        ],
        ['wrapped response', candle(), { data: { response: [] } }],
        ['invalid response', candle(), { data: { data: 17.5 } }],
        ['field error', candle(), { throwGet: 't' }],
        ['request rejection', candle(), { rejectPost: true }],
    ])
        await add('fetchOHLCV', label, [opts], c);
    for (const [label, opts, c] of [
        ['both', {}, {}],
        ['funding', { incomeType: 'FUNDING_FEE' }, {}],
        ['fill only', { incomeType: 'REALIZED_PNL' }, {}],
        ['symbol filter', { symbol: 'ETH-PERP' }, {}],
        ['explicit times/default post body', { startTime: 138, endTime: 150 }, { bodyUndefined: true }],
        ['cap callback', {}, { capHit: true }],
        ['body override', {}, { bodyOverride: { user: 'override-user', type: 'override-type' } }],
        ['public', {}, { publicOnly: true }],
        ['no user', {}, { options: { vault: '', applicationId: '' } }],
        ['post reject', {}, { rejectPost: true }],
    ])
        await add('fetchIncomeHistory', label, [opts], c);
    for (const [label, opts, c] of [
        ['latest orders', {}, {}],
        ['filters', { symbol: 'BTC-PERP', from: currentTime - 50, to: currentTime + 1, limit: 1 }, {}],
        ['zero filters', { symbol: '', from: 0, to: 0, limit: 0 }, {}],
        ['null orders', {}, { nullOrders: true }],
        ['nonarray', {}, { data: 'rejected-150' }],
        ['snapshot cap', {}, { snapshotCap: true }],
        ['public', {}, { publicOnly: true }],
        ['reject', {}, { rejectPost: true }],
    ])
        await add('fetchHistoricalOrders', label, [opts], c);
    for (const [label, opts, c] of [
        ['derived', { accountName: 'acct-150' }, {}],
        ['filter', { symbol: 'ETH-PERP' }, {}],
        ['explicit times', { from: 138, to: 150 }, {}],
        ['default account', {}, {}],
        ['public', {}, { publicOnly: true }],
        ['no user', {}, { options: { vault: '', applicationId: '' } }],
        ['reject', {}, { rejectPost: true }],
        ['undefined options', undefined, {}],
    ])
        await add('deriveClosedPositions', label, [opts], c);
    for (const [label, type, c] of [
        ['undefined income', undefined, {}],
        ['funding', 'FUNDING_FEE', {}],
        ['false income', false, {}],
        ['zero income', 0, {}],
        ['range rejects', 'REALIZED_PNL', { rejectRange: true }],
        ['callee error', 'REALIZED_PNL', { throwCall: 'fetchIncomeHistory' }],
    ])
        await add('fetchIncomeHistoryRange', label, [138, 150, type], c);
    const exec = (extra = {}) => ({
        symbol: 'BTC-PERP',
        from: currentTime - 1000,
        to: currentTime,
        limit: 2,
        ...extra,
    });
    for (const [label, opts, c] of [
        ['last wins and fields', exec(), {}],
        ['seconds', exec({ from: (currentTime - 1000) / 1000, to: currentTime / 1000 }), {}],
        ['windows', exec({ from: currentTime - 8 * 86400000, limit: 500 }), {}],
        ['cap lookback', exec({ from: currentTime - 40 * 86400000 }), {}],
        ['defaults', { symbol: 'BTC-PERP' }, {}],
        ['public', exec(), { publicOnly: true }],
        ['missing symbol', {}, {}],
        ['missing market', exec({ symbol: 'UNKNOWN' }), { missingMarket: true }],
        ['no app', exec(), { options: { applicationId: '' } }],
        ['inverted', exec({ from: currentTime + 1 }), {}],
        ['invalid timestamps', exec({ from: NaN, to: NaN }), {}],
        ['unknown coin', exec(), { data: [fill({ coin: 'ETH' })] }],
        ['order filter', exec({ orderId: '151' }), {}],
        ['empty order accepted', exec({ orderId: '151' }), { data: [fill({ oid: '', orderId: '' })] }],
        ['bad price', exec(), { data: [fill({ px: '0' })] }],
        ['bad side', exec(), { data: [fill({ side: 'unknown' })] }],
        ['nonbool maker', exec(), { data: [fill({ crossed: 0, builderFee: undefined, closedPnl: undefined })] }],
        ['no tid', exec(), { data: [fill({ tid: null, side: 'SELL' })] }],
        ['request reject', exec(), { rejectPost: true }],
        ['nonarray', exec(), { data: 'bad' }],
    ])
        await add('fetchHistoricalFills', label, [opts], c);
    for (const [label, ids, c] of [
        ['sum ids', ['138'], {}],
        ['empty ids', [], {}],
        ['different', ['150'], {}],
        ['numeric ids', [138], {}],
        ['nan accumulation', ['138'], { data: [fill({ sz: 'NaN' }), fill({ sz: '4.75' })] }],
        ['negative and zero', ['138'], { data: [fill({ sz: '-0' }), fill({ sz: '-4.75' })] }],
        ['invalid response', ['138'], { data: null }],
        ['request rejection', ['138'], { rejectPost: true }],
    ])
        await add('fetchFillsForOrderIds', label, [ids], c);
    const awaitExpectation = [0, 8, 8, 10, 26, 7, 10, 5, 20, 7];
    const expectation = [8, 8, 8, 10, 10, 8, 8, 6, 20, 8];
    metadata.roots.forEach(([_, name], i) =>
        assert.equal(
            cases.filter((c) => c.name === name).reduce((sum, c) => sum + c.expectedAwaits, 0),
            awaitExpectation[i],
            name + ' fixed await floor',
        ),
    );
    assert.equal(
        awaitExpectation.reduce((a, b) => a + b, 0),
        101,
    );
    assert.equal(cases.length, 94);
    metadata.roots.forEach(([_, name], i) =>
        assert.equal(cases.filter((c) => c.name === name).length, expectation[i], name),
    );
    console.log(
        JSON.stringify({
            names,
            fieldNames,
            effectNames,
            cases,
            expectation,
            iterationStop: 93,
            callbackKinds: metadata.callbacks.map((c) => [c.kind, c.async]),
            scope: 'ten REST bodies and27owned callback bodies, explicit library/importedhelper/virtualcallee contracts; eager async/task/await cutpoints, finite fixtures, no universal JS/eventloop/live proof',
        }),
    );
})().catch((error) => {
    console.error(error);
    process.exitCode = 1;
});
