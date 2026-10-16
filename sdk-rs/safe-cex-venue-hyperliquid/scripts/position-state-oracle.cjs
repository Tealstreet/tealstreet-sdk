'use strict';
const fs = require('node:fs'),
    path = require('node:path'),
    vm = require('node:vm'),
    assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript'),
    root = path.resolve(__dirname, '../../..');
const metadata = require('./position-state-generate.cjs');
const sourceNames = {
    core: metadata.roots.filter((r) => r[0] === 'core').map((r) => r[1]),
    private: metadata.roots.filter((r) => r[0] === 'private').map((r) => r[1]),
};
const names = metadata.roots.map((r) => r.join('.'));
let g, ids, tags, nextId, calls, protos;
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
    __construct: (constructor, ...args) =>
        op('construct', { constructor: native(constructor), args: args.map(native) }, () => {
            if (g.throwConstruct) throw g.error;
            return tag(Reflect.construct(constructor, args), 'map');
        }),
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
        const e = { op: 'call', args: { function: native(fn), receiver: native(receiver), args: args.map(native) } };
        calls.push(e);
        try {
            if (g.throwCall !== undefined && tags.get(fn) === g.throwCall) throw g.error;
            if (['filter', 'map', 'find', 'some', 'sort'].includes(tags.get(fn))) {
                e.callbacks = [];
                const callback = args[0];
                args[0] = function (...inputs) {
                    const cb = { inputs: inputs.map(native) };
                    e.callbacks.push(cb);
                    try {
                        const result = Reflect.apply(callback, undefined, inputs);
                        cb.result = native(result);
                        return result;
                    } catch (error) {
                        cb.throw = native(error);
                        throw error;
                    }
                };
            }
            const result = Reflect.apply(fn, receiver, args);
            if (Array.isArray(result)) {
                tag(result, 'array');
                e.elements = Array.from(result, native);
                e.prototype = native(Object.getPrototypeOf(result));
            }
            e.result = native(result);
            if (effectNames.includes(tags.get(fn))) e.stateAfter = g.state();
            return result;
        } catch (error) {
            e.throw = native(error);
            throw error;
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
    __predicate: (kind, captures, fn) =>
        op('positionCallback', { kind, captures: captures.map(native) }, () => tag(fn, 'positionCallback:' + kind)),
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
    const visit = (n) => {
        if (ts.isTypeNode(n)) return n;
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
        if (ts.isVariableDeclaration(n) && ts.isObjectBindingPattern(n.name) && n.name.elements.length > 1) {
            return f.updateVariableDeclaration(
                n,
                f.createArrayBindingPattern(
                    n.name.elements.map((e) => f.createBindingElement(undefined, undefined, e.name)),
                ),
                n.exclamationToken,
                n.type,
                call('__binding', [
                    ts.visitNode(n.initializer, visit),
                    f.createArrayLiteralExpression(n.name.elements.map((e) => f.createStringLiteral(e.name.getText()))),
                ]),
            );
        }
        if (ts.isVariableDeclaration(n) && ts.isObjectBindingPattern(n.name)) {
            assert.equal(n.name.elements.length, 1);
            const element = n.name.elements[0];
            assert.equal(element.name.getText(), 'data');
            return f.updateVariableDeclaration(
                n,
                f.createIdentifier('data'),
                n.exclamationToken,
                n.type,
                call('__get', [ts.visitNode(n.initializer, visit), f.createStringLiteral('data')]),
            );
        }
        if (ts.isVariableDeclaration(n) && ts.isArrayBindingPattern(n.name))
            return f.updateVariableDeclaration(
                n,
                n.name,
                n.exclamationToken,
                n.type,
                call('__pair', [ts.visitNode(n.initializer, visit)]),
            );
        if (ts.isRegularExpressionLiteral(n)) return call('__regex', [n]);
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
        if (ts.isCallExpression(n) && ts.isIdentifier(n.expression) && globalNames.has(n.expression.text))
            return call('__call', [
                call('__global', [f.createStringLiteral(n.expression.text)]),
                f.createIdentifier('undefined'),
                ...n.arguments.map((a) => ts.visitNode(a, visit)),
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
            const args = n.arguments.map((a) => {
                if (!ts.isArrowFunction(a)) return ts.visitNode(a, visit);
                const c = metadata.callbacks.find((c) => c.node.getText() === a.getText() && c.root === activeRoot);
                assert(c, 'callback metadata');
                return call('__predicate', [
                    f.createStringLiteral(c.kind),
                    f.createArrayLiteralExpression(
                        c.captures.map((v) => (v === 'this_value' ? f.createThis() : f.createIdentifier(v))),
                    ),
                    ts.visitNode(a, visit),
                ]);
            });
            statements.push(
                variable(fn, call('__get', [recv, f.createStringLiteral(n.expression.name.text)])),
                f.createReturnStatement(call('__call', [fn, recv, ...args])),
            );
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
        if (ts.isIdentifier(n) && globalNames.has(n.text)) return call('__global', [f.createStringLiteral(n.text)]);
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
for (const owner of ['core', 'private']) {
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
    const constant =
        owner === 'private'
            ? cls.members.find((m) => m.name?.getText(ast) === 'CORE_FILL_POSITION_EPSILON').getText(ast)
            : '';
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
const fieldNames = [
    'lastCoreClearinghouseStateServerTime',
    'lastCoreClearinghouseStateAt',
    'latestCorePerpsUpnl',
    'positions',
    'orders',
    'balance',
];
const effectNames = [
    'log',
    'addOrUpdatePositionsFull',
    'addOrUpdatePositions',
    'addOrUpdatePosition',
    'updatePosition',
    'update',
    'removeOrder',
    'applyCorePerpsBalanceSnapshot',
    'syncBalanceFromLivePositions',
    'requestUnifiedSpotCollateralRefresh',
    'applyBalanceFromCurrentMode',
    'trackProcessedRealizedPnlFill',
    'setRealizedPnlForPosition',
    'incrementRealizedPnlForPosition',
    'applyRealizedPnlToCurrentPosition',
    'applyOptimisticCorePositionFromFill',
    'scheduleHip3PositionRefresh',
    'suppressCancelledOrder',
    'forgetLimitOrderTimeInForce',
    'removePendingHip3Order',
    'matchPrePlacement',
    'forgetPlacementRecord',
    'forgetPrePlacement',
    'scheduleOrderRefresh',
    'emit',
    'post',
];
const cases = [];
const ap = (coin = 'BTC', szi = '4.75', extra = {}) => ({
    position: {
        coin,
        szi,
        entryPx: '17.5',
        positionValue: '91.25',
        leverage: { value: '7', type: 'cross' },
        unrealizedPnl: '3.5',
        liquidationPx: '12.25',
        marginUsed: '2.75',
        ...extra,
    },
});
const market = (id = 'BTC', extra = {}) => ({
    id,
    symbol: id + '/USD',
    isSpot: false,
    info: id.includes(':') ? { hip3Dex: id.split(':')[0] } : {},
    ...extra,
});
const fill = (extra = {}) => ({
    coin: 'BTC',
    tid: 150,
    oid: 138,
    dir: 'Close Long',
    time: 17.5,
    fee: '0.25',
    closedPnl: '3.5',
    startPosition: '4.75',
    side: 'A',
    px: '91.25',
    sz: '1.25',
    ...extra,
});
const order = (extra = {}) => ({
    coin: 'BTC',
    oid: 150,
    side: 'B',
    origSz: '4.75',
    sz: '1.25',
    limitPx: '91.25',
    tif: 'Gtc',
    ...extra,
});
const position = (extra = {}) => ({
    id: 'BTC/USD:long',
    symbol: 'BTC/USD',
    side: 'long',
    contracts: 4.75,
    entryPrice: 17.5,
    unrealizedPnl: 3.5,
    liquidationPrice: 12.25,
    leverage: 7,
    marginType: 'cross',
    positionMode: 'oneway',
    ...extra,
});
async function add(name, label, args, config = {}) {
    ids = new WeakMap();
    tags = new WeakMap();
    nextId = 1;
    calls = [];
    tag(Object.prototype, 'defaultPrototype');
    tag(Array.prototype, 'arrayPrototype');
    const owner = metadata.roots.find((r) => r[1] === name)[0],
        e = tag(new classes[owner](), 'exchange'),
        selected = e[name],
        effects = [],
        error = tag(new Error('native rejection'), 'error');
    const parent = owner === 'core' ? e : tag({}, 'parent');
    e.parent = parent;
    const store = tag(
        {
            marketsById: { BTC: market(), 'xyz:BTC': market('xyz:BTC'), SPOT: market('SPOT', { isSpot: true }) },
            marketsBySymbol: {},
            positions: tag(
                config.positions ?? [position(), position({ id: 'xyz:BTC/USD:long', symbol: 'xyz:BTC/USD' })],
                'positions',
            ),
            orders: tag(
                config.orders ?? [{ id: '150', type: 'limit', price: 17.5, status: 'creating', timeInForce: 'IOC' }],
                'orders',
            ),
            balance: tag(config.balance ?? { total: 501.25, used: 91.25, upnl: 3.5, free: 400 }, 'balance'),
            tickersBySymbol: config.tickers ?? { 'BTC/USD': { last: 91.25 } },
        },
        'store',
    );
    for (const m of Object.values(store.marketsById)) store.marketsBySymbol[m.symbol] = m;
    if (config.marketId) store.marketsById.BTC = market(config.marketId);
    e.store = store;
    parent.store = store;
    parent.marketsLowerCaseMap = new Map([
        ['xyz:btc', store.marketsById['xyz:BTC']],
        ['btc', store.marketsById.BTC],
    ]);
    const fakes = tag(
        config.fakes ?? [
            position({ contracts: 0 }),
            position({ id: 'xyz:BTC/USD:long', symbol: 'xyz:BTC/USD', contracts: 0 }),
        ],
        'fakePositions',
    );
    const raw = {
        lastCoreClearinghouseStateServerTime: config.lastTime ?? 10,
        lastCoreClearinghouseStateAt: 0,
        latestCorePerpsUpnl: 0,
    };
    for (const n of fieldNames.slice(0, 3))
        Object.defineProperty(e, n, {
            get() {
                return raw[n];
            },
            set(v) {
                raw[n] = v;
            },
        });
    const state = () => fieldNames.map((n) => [n, tree(n in raw ? raw[n] : store[n])]);
    g = { error, ...config, state, afterAwait() {} };
    function virtual(target, n, fn) {
        target[n] = tag(function (...a) {
            effects.push({ name: n, receiver: native(this), args: a.map(tree) });
            return fn ? fn(...a) : undefined;
        }, n);
    }
    const simple = (target, n, fn) =>
        (target[n] = tag(function (...a) {
            return fn(...a);
        }, n));
    for (const n of effectNames) if (!['post', 'emit'].includes(n)) virtual(parent, n);
    virtual(store, 'addOrUpdatePositionsFull', (p) => (store.positions = p));
    virtual(store, 'addOrUpdatePositions', (p) => {
        const by = new Map(store.positions.map((p) => [p.id, p]));
        for (const q of p) by.set(q.id, q);
        store.positions = tag(Array.from(by.values()), 'positions');
    });
    virtual(store, 'addOrUpdatePosition', (p) => {
        store.positions = tag([...store.positions.filter((q) => q.symbol !== p.symbol), p], 'positions');
    });
    virtual(store, 'updatePosition', (p, patch) => Object.assign(p, patch));
    virtual(store, 'update', (patch) => Object.assign(store, patch));
    virtual(store, 'removeOrder', ({ id }) => {
        store.orders = tag(
            store.orders.filter((o) => o.id !== id),
            'orders',
        );
    });
    virtual(store, 'addOrUpdateOrder', (o) => {
        store.orders = tag([...store.orders.filter((q) => q.id !== o.id), o], 'orders');
    });
    effectNames.includes('addOrUpdateOrder') || effectNames.push('addOrUpdateOrder');
    simple(parent, 'getFakePositions', () => fakes);
    simple(parent, '_getSymbolConfigurationFromAssetData', () =>
        tag(
            config.symbolConfig ?? { leverage: 7, positionMode: config.mode ?? 'oneway', marginType: 'cross' },
            'symbolConfig',
        ),
    );
    simple(e, 'getMessageTimestamp', () => (Object.hasOwn(config, 'timestamp') ? config.timestamp : 17.5));
    simple(e, 'resolvePositionMode', () => config.mode ?? 'oneway');
    simple(e, 'getRealizedPnlForPosition', () => (Object.hasOwn(config, 'realized') ? config.realized : 91.25));
    simple(e, 'extractCoreBalanceSummary', () => config.summary ?? null);
    simple(e, 'resolveSymbolFromCoin', (coin) => coin + '/USD');
    simple(e, 'safeParseNumber', (v) => Number(v) || 0);
    simple(e, 'getFillPositionSide', (dir) => (dir.includes('long') ? 'long' : dir.includes('short') ? 'short' : null));
    for (const n of [
        'trackProcessedRealizedPnlFill',
        'setRealizedPnlForPosition',
        'incrementRealizedPnlForPosition',
        'applyRealizedPnlToCurrentPosition',
        'applyOptimisticCorePositionFromFill',
    ])
        virtual(e, n);
    simple(
        e,
        'collectHip3DexesFromFills',
        (fills) => new Set(fills.filter((f) => f.coin.includes(':')).map((f) => f.coin.split(':')[0])),
    );
    e.processedRealizedPnlFillIds = new Set(config.processed ?? []);
    parent.emitter = tag({}, 'emitter');
    virtual(parent.emitter, 'emit');
    simple(parent, 'isOrderCancelled', (id) => (config.cancelled ?? []).includes(id));
    simple(parent, 'getPendingHip3Order', () => config.pending ?? null);
    simple(parent, 'getPlacedLimitOrderTimeInForce', () => config.placedTif);
    virtual(parent, 'matchPrePlacement', () => config.preplacement ?? null);
    simple(parent, 'getPlacementRecord', () => (config.record ? tag({ ...config.record }, 'record') : undefined));
    simple(parent, 'matchPrePlacementTrigger', () => (config.stopEntry ? tag({ ...config.stopEntry }, 'stopEntry') : undefined));
    e._deadOrders = new Set(config.dead ?? []);
    e.isPublicDataOnly = config.publicOnly ?? false;
    simple(e, 'shouldUseSpotCollateralBalance', () => config.spotCollateral ?? false);
    e.hasSpotCollateralSnapshot = config.spotSnapshot ?? false;
    e.options = tag({ vault: '', applicationId: 'main-wallet' }, 'options');
    e.xhr = tag({}, 'xhr');
    virtual(e.xhr, 'post', () =>
        config.rejectPost
            ? Promise.reject(error)
            : Promise.resolve(
                  tag(
                      { data: Object.hasOwn(config, 'data') ? config.data : { assetPositions: [ap('xyz:BTC')] } },
                      'response',
                  ),
              ),
    );
    const seen = new Set(config.seen ?? []);
    Object.assign(globals, {
        Object: tag(Object, 'Object'),
        Number: tag(Number, 'Number'),
        Array: tag(Array, 'Array'),
        Boolean: tag(Boolean, 'Boolean'),
        String: tag(String, 'String'),
        Math: tag(Math, 'Math'),
        Map: tag(Map, 'Map'),
        Date: tag({ now: () => 150 }, 'Date'),
        parseFloat: tag(parseFloat, 'parseFloat'),
        PositionSide: tag({ Long: 'long', Short: 'short' }, 'PositionSide'),
        PositionMode: tag({ OneWay: 'oneway', Hedged: 'hedged' }, 'PositionMode'),
        MarginType: tag({ Cross: 'cross', Isolated: 'isolated' }, 'MarginType'),
        OrderSide: tag({ Buy: 'buy', Sell: 'sell' }, 'OrderSide'),
        OrderStatus: tag({ Creating: 'creating', Updating: 'updating', Open: 'open' }, 'OrderStatus'),
        OrderType: tag({ Limit: 'limit' }, 'OrderType'),
        virtualClock: tag({ getCurrentTimeMs: () => 138 }, 'virtualClock'),
        LogSeverity: tag({ Debug: 'debug', Info: 'info', Warning: 'warning' }, 'LogSeverity'),
        INFO_TYPES: tag(infoTypes, 'INFO_TYPES'),
        ENDPOINTS: tag(endpoints, 'ENDPOINTS'),
        HyperliquidPrivateWebSocket: tag(classes.private, 'HyperliquidPrivateWebSocket'),
    });
    globals.createPositionId = tag((symbol, side, mode) => `${symbol}:${side}:${mode}`, 'createPositionId');
    globals.inferHyperliquidOrderTypeAndPrice = tag(
        (o) =>
            tag(
                { type: o.orderType ?? 'limit', price: parseFloat(o.triggerPx ?? o.limitPx) || 0, source: 'inferred' },
                'inferred',
            ),
        'inferHyperliquidOrderTypeAndPrice',
    );
    globals.parseHyperliquidOrderTimeInForce = tag(
        (t) => (t === 'Gtc' ? 'GTC' : undefined),
        'parseHyperliquidOrderTimeInForce',
    );
    // Same contract as hyperliquid.cloid.ts: a lowercase 0x + 32-hex string, else undefined.
    globals.readHyperliquidCloid = tag(
        (v) => (typeof v === 'string' && /^0x[0-9a-f]{32}$/.test(v) ? v : undefined),
        'readHyperliquidCloid',
    );
    globals.hasSeenFill = tag((id) => seen.has(id), 'hasSeenFill');
    globals.markFillSeen = tag((id) => seen.add(id), 'markFillSeen');
    globals.uniqBy = tag((xs, key) => {
        const seen = new Set();
        return xs.filter((x) => {
            const id = x[key];
            if (seen.has(id)) return false;
            seen.add(id);
            return true;
        });
    }, 'uniqBy');
    if (config.passMarkets) args[args.length - 1] = store.marketsById;
    args = args.map((v, i) => (v && typeof v === 'object' && !tags.has(v) ? tag(v, 'argument' + i) : v));
    const input = native(e),
        inputArgs = args.map(native),
        before = state();
    const seedArrays = [store.positions, store.orders, fakes, ...args.filter(Array.isArray)].map((value) => ({
        value: native(value),
        elements: Array.from(value, native),
    }));
    let result;
    try {
        result = { value: tree(await Reflect.apply(selected, e, args)) };
    } catch (error) {
        result = { throw: native(error) };
    }
    cases.push({
        name,
        label,
        input,
        args: inputArgs,
        calls,
        initial: before,
        final: state(),
        seedArrays,
        effects,
        result,
        expectedAwaits: calls.filter((c) => c.op === 'await').length,
        disposeAt: null,
    });
}
(async () => {
    const data = () => ({
        assetPositions: [ap(), ap('xyz:BTC'), ap('UNKNOWN'), ap('BTC', '-0', { marginUsed: null })],
    });
    for (const [label, args, c] of [
        ['incremental', [data(), false], {}],
        ['authoritative', [data(), true], {}],
        ['stale', [data(), true], { lastTime: 91.25 }],
        ['null timestamp', [data(), true], { timestamp: null }],
        ['empty', [{ assetPositions: [] }, true], {}],
        ['no positions', [{}, false], {}],
        ['realized absent', [data(), false], { realized: undefined }],
        ['summary after insertion', [data(), false], { summary: { accountValue: 501.25, totalMarginUsed: 91.25 } }],
        ['getter failure', [data(), false], { throwGet: 'entryPx' }],
        ['full update failure', [data(), true], { throwCall: 'addOrUpdatePositionsFull' }],
    ])
        await add('applyCoreClearinghouseState', label, args, c);
    for (const [label, args, c] of [
        ['normalized', ['XYZ', data()], {}],
        ['empty dex', ['', data()], {}],
        ['invalid positions', ['xyz', { assetPositions: 0 }], {}],
        ['empty', ['xyz', { assetPositions: [] }], {}],
        ['lowercase lookup', ['xyz', { assetPositions: [ap('XYZ:BTC')] }], {}],
        ['unknown', ['xyz', { assetPositions: [ap('unknown')] }], {}],
        ['null', ['xyz', null], {}],
        ['no realized', ['xyz', data()], { realized: undefined }],
        [
            'isolated hedged',
            [
                'xyz',
                {
                    assetPositions: [
                        ap('BTC', '-4.75', { leverage: { value: '7', type: 'isolated' }, marginUsed: '0' }),
                    ],
                },
            ],
            { mode: 'hedged' },
        ],
        ['callee failure', ['xyz', data()], { throwCall: 'addOrUpdatePositionsFull' }],
    ])
        await add('handleHip3ClearinghouseState', label, args, c);
    const fills = () => [
        fill({ tid: 152, time: 20 }),
        fill({ tid: 150, time: 17.5 }),
        fill({ coin: 'xyz:BTC', tid: 151, time: 17.5, dir: 'Open Short', startPosition: 0 }),
    ];
    for (const [label, d, c] of [
        ['sorted live', { fills: fills(), isSnapshot: false }, {}],
        ['snapshot', { fills: fills(), isSnapshot: true }, {}],
        ['empty', { fills: [] }, {}],
        ['missing', {}, {}],
        ['processed', { fills: [fill()] }, { processed: ['150'] }],
        ['seen', { fills: [fill()] }, { seen: ['150'] }],
        ['nonclose', { fills: [fill({ dir: 'Other' })] }, {}],
        ['lowercase', { fills: [fill({ coin: 'XYZ:BTC' })] }, {}],
        ['getter failure', { fills: [fill()] }, { throwGet: 'fee' }],
        ['notification failure', { fills: [fill()] }, { throwCall: 'emit' }],
    ])
        await add('handleFills', label, [d], c);
    for (const [label, f, c] of [
        ['reduce', fill(), {}],
        ['weighted add', fill({ side: 'B' }), {}],
        ['flip', fill({ sz: '7' }), {}],
        ['close', fill({ sz: '4.75' }), {}],
        ['open new', fill({ side: 'B', startPosition: '0' }), { positions: [] }],
        ['missing coin', null, {}],
        ['HIP3', fill({ coin: 'xyz:BTC' }), {}],
        ['spot', fill({ coin: 'SPOT' }), {}],
        ['hedged', fill(), { mode: 'hedged', positions: [position({ positionMode: 'hedged' })] }],
        ['invalid start', fill({ startPosition: 'bad' }), {}],
        ['zero price', fill({ px: '0' }), {}],
        ['epsilon', fill({ startPosition: '1.25000000001' }), {}],
    ])
        await add('applyOptimisticCorePositionFromFill', label, [f, undefined], { ...c, passMarkets: true });
    const updates = (o = order(), status = 'open') => [{ order: o, status }];
    for (const [label, u, c] of [
        ['core open', updates(), {}],
        ['removed', updates(order(), 'filled'), {}],
        ['cancelled', updates(), { cancelled: ['150'] }],
        ['pending HIP3', updates(order({ coin: 'xyz:BTC' })), { pending: { type: 'stop-loss', price: 17.5 } }],
        ['HIP3 preserved', updates(order({ coin: 'xyz:BTC' })), {}],
        [
            'preplacement',
            updates(order({ coin: 'xyz:BTC', oid: 151 })),
            { preplacement: { type: 'take-profit', price: 17.5 } },
        ],
        ['placeholder', updates(order({ coin: 'xyz:BTC', oid: 151 })), {}],
        ['core protective deferral', updates(order({ oid: 151, reduceOnly: true, origSz: 0 })), {}],
        ['core protective preserved', updates(order({ reduceOnly: true, origSz: 0 })), {}],
        ['unknown market', updates(order({ coin: 'unknown' })), {}],
        ['lowercase', updates(order({ coin: 'XYZ:BTC' })), {}],
        ['unknown status', updates(order(), 'unknown'), {}],
        ['empty', [], {}],
        ['null', null, {}],
        ['iterator step error', updates(), { throwNext: 1 }],
        ['body error closes', updates(), { throwGet: 'status' }],
        [
            'cloid record',
            updates(order({ oid: 152, cloid: '0x0123456789abcdef0123456789abcdef' })),
            { record: { type: 'stop-loss', price: 17.5, side: 'buy', amount: 4.75, reduceOnly: true, symbol: 'BTC/USD', placedAt: 100 } },
        ],
        ['cloid without record', updates(order({ oid: 152, cloid: '0x0123456789abcdef0123456789abcdef' })), {}],
        ['removed with cloid', updates(order({ cloid: '0x0123456789abcdef0123456789abcdef' }), 'filled'), {}],
        ['core stop entry', updates(order({ oid: 153 })), { stopEntry: { type: 'stop-loss', price: 21.5 } }],
    ])
        await add('handleOrderUpdates', label, [u], c);
    for (const [label, coin, side, c] of [
        ['match', 'BTC', 'long', {}],
        ['zero', 'BTC', 'long', { realized: 0 }],
        ['null pnl', 'BTC', 'long', { realized: null }],
        ['undefined', 'BTC', 'long', { realized: undefined }],
        ['no match', 'UNKNOWN', 'long', {}],
        ['wrong side', 'BTC', 'short', {}],
        ['symbol error', 'BTC', 'long', { throwGet: 'symbol' }],
        ['update error', 'BTC', 'long', { throwCall: 'updatePosition' }],
    ])
        await add('applyRealizedPnlToCurrentPosition', label, [coin, side, undefined], { ...c, passMarkets: true });
    for (const [label, ms, c] of [
        ['oneway', [market(), market('SPOT', { isSpot: true })], {}],
        ['hedged', [market(), market('ETH')], { mode: 'hedged' }],
        ['empty', [], {}],
        [
            'config overrides',
            [market()],
            { symbolConfig: { positionMode: 'oneway', id: 'override', side: 'short', contracts: 91.25 } },
        ],
        ['truthy spot', [market('SPOT', { isSpot: 1 })], {}],
        ['other mode', [market()], { mode: null }],
        ['getter error', [market()], { throwGet: 'symbol' }],
        ['map error', [market()], { throwCall: 'map' }],
    ])
        await add('generateFakePositionsFromMarkets', label, [ms], c);
    for (const [label, symbols, c] of [
        ['long', undefined, {}],
        ['short', undefined, { positions: [position({ side: 'short' })] }],
        ['updated', new Set(['BTC/USD']), {}],
        ['not updated', new Set(['NONE']), {}],
        ['fallback', undefined, { tickers: {} }],
        ['no open', undefined, { positions: [], balance: { total: 501.25, used: 91.25, upnl: 0, free: 410 } }],
        ['public', undefined, { publicOnly: true }],
        ['invalid wallet', undefined, { balance: { total: 'NaN', used: 91.25 } }],
        ['spot collateral', undefined, { spotCollateral: true, spotSnapshot: true }],
        ['unchanged', undefined, { balance: { total: 501.25, used: 91.25, upnl: 350.3125 + 3.5, free: 763.8125 } }],
        ['iterator failure', undefined, { throwNext: 1 }],
        ['body failure closes', undefined, { throwGet: 'contracts' }],
    ])
        await add('syncBalanceFromLivePositions', label, [symbols], c);
    for (const [label, c] of [
        ['nonempty', {}],
        ['empty', { data: { assetPositions: [] } }],
        ['null', { data: null }],
        ['zero', { data: { assetPositions: [ap('xyz:BTC', '0')] } }],
        ['unknown', { data: { assetPositions: [ap('unknown')] } }],
        ['lowercase', { data: { assetPositions: [ap('XYZ:BTC')] } }],
        [
            'raw leverage',
            {
                data: {
                    assetPositions: [
                        ap('xyz:BTC', '-4.75', {
                            leverage: { value: '7', type: 'isolated' },
                            liquidationPx: null,
                            marginUsed: 0,
                        }),
                    ],
                },
            },
        ],
        ['reject', { rejectPost: true }],
        ['body error', { throwGet: 'entryPx' }],
        ['options outside catch', { throwGet: 'options' }],
    ])
        await add('fetchHip3Positions', label, ['xyz'], c);
    for (const [label, os, c] of [
        ['native decoded fields', [order()], {}],
        ['duplicates', [order(), order({ side: 'A' })], {}],
        ['dead', [order()], { dead: ['150'] }],
        ['cancelled', [order()], { cancelled: ['150'] }],
        ['unknown', [order({ coin: 'UNKNOWN' })], {}],
        ['lowercase', [order({ coin: 'XYZ:BTC' })], {}],
        ['empty', [], {}],
        ['field error', [order()], { throwGet: 'origSz' }],
        ['inference error', [order()], { throwCall: 'inferHyperliquidOrderTypeAndPrice' }],
        ['null input', null, {}],
    ])
        await add('parseOpenOrders', label, [os], c);
    const cloid = '0x0123456789abcdef0123456789abcdef';
    for (const [label, row, status, c] of [
        ['cloid', order({ cloid }), 'open', {}],
        ['null cloid', order({ cloid: null }), 'open', {}],
        ['uppercase cloid', order({ cloid: cloid.toUpperCase() }), 'open', {}],
        ['unknown market', order({ coin: 'UNKNOWN', cloid }), 'open', {}],
        ['lowercase hip3', order({ coin: 'XYZ:BTC', cloid }), 'open', {}],
        ['closed status', order({ cloid }), 'closed', {}],
    ])
        await add('mapOpenOrderRow', label, [row, status], c);
    const expectation = [10, 10, 10, 12, 20, 8, 8, 12, 10, 10, 6];
    assert.equal(cases.length, 116);
    metadata.roots.forEach(([owner, name], i) =>
        assert.equal(cases.filter((c) => c.name === name).length, expectation[i], name),
    );
    console.log(
        JSON.stringify({
            names,
            fieldNames,
            effectNames,
            cases,
            expectation,
            iterationStop: 115,
            scope: 'eleven bodies/owned callbacks; fixture virtual callees and ECMAScript primitives; no full classes/live exchange/general async proof',
        }),
    );
})().catch((error) => {
    console.error(error);
    process.exitCode = 1;
});
