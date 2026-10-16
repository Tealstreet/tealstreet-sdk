'use strict';
// Deterministic body lowering. Generated Rust keeps source branches, reads and callbacks.
// This generator is a maintenance aid; the oracle runs the original TypeScript bodies.
const fs = require('node:fs'),
    path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const dir = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/hyperliquid');
const roots = [
    ['private', 'applyCoreClearinghouseState'],
    ['private', 'handleHip3ClearinghouseState'],
    ['private', 'handleFills'],
    ['private', 'applyOptimisticCorePositionFromFill'],
    ['private', 'handleOrderUpdates'],
    ['private', 'applyRealizedPnlToCurrentPosition'],
    ['core', 'generateFakePositionsFromMarkets'],
    ['core', 'syncBalanceFromLivePositions'],
    ['core', 'fetchHip3Positions'],
    ['core', 'parseOpenOrders'],
    ['core', 'mapOpenOrderRow'],
];
const globals = new Set([
    'Object',
    'Number',
    'Boolean',
    'String',
    'Array',
    'Math',
    'Map',
    'Date',
    'PositionSide',
    'PositionMode',
    'MarginType',
    'OrderSide',
    'OrderStatus',
    'OrderType',
    'LogSeverity',
    'INFO_TYPES',
    'ENDPOINTS',
    'HyperliquidPrivateWebSocket',
    'virtualClock',
    'parseFloat',
    'createPositionId',
    'inferHyperliquidOrderTypeAndPrice',
    'parseHyperliquidOrderTimeInForce',
    'readHyperliquidCloid',
    'hasSeenFill',
    'markFillSeen',
    'uniqBy',
]);
const snake = (s) => s.replace(/[A-Z]/g, (c) => '_' + c.toLowerCase());
const quote = JSON.stringify,
    text = (s) => `lit(h, ${quote(s)})`;
const parsed = {};
for (const [k, file] of [
    ['core', 'hyperliquid.exchange.ts'],
    ['private', 'hyperliquid.ws-private.ts'],
]) {
    const ast = ts.createSourceFile(file, fs.readFileSync(path.join(dir, file), 'utf8'), ts.ScriptTarget.Latest, true);
    parsed[k] = { ast, cls: ast.statements.find(ts.isClassDeclaration) };
}
const callbacks = [];
let current, mutated;
function walk(n, fn) {
    fn(n);
    ts.forEachChild(n, (c) => walk(c, fn));
}
function collectMutated(n) {
    const set = new Set();
    walk(n, (x) => {
        if (
            ts.isBinaryExpression(x) &&
            [ts.SyntaxKind.EqualsToken, ts.SyntaxKind.PlusEqualsToken].includes(x.operatorToken.kind) &&
            ts.isIdentifier(x.left)
        )
            set.add(x.left.text);
        if (ts.isPostfixUnaryExpression(x) && ts.isIdentifier(x.operand)) set.add(x.operand.text);
    });
    return set;
}
function callbackMeta(n) {
    let found = callbacks.find((c) => c.node === n);
    if (found) return found;
    const local = new Set(n.parameters.map((p) => p.name.text));
    walk(n.body, (x) => {
        if (ts.isVariableDeclaration(x) && ts.isIdentifier(x.name)) local.add(x.name.text);
    });
    const captures = [];
    walk(n.body, (x) => {
        let name;
        if (x.kind === ts.SyntaxKind.ThisKeyword) name = 'this_value';
        else if (ts.isIdentifier(x)) {
            const p = x.parent;
            if (
                (ts.isPropertyAccessExpression(p) && p.name === x) ||
                (ts.isPropertyAssignment(p) && p.name === x) ||
                (ts.isVariableDeclaration(p) && p.name === x) ||
                (ts.isParameter(p) && p.name === x) ||
                ts.isTypeNode(p)
            )
                return;
            name = x.text;
            if (local.has(name) || globals.has(name) || ['undefined'].includes(name)) return;
        }
        if (name && !captures.includes(name)) captures.push(name);
    });
    found = {
        node: n,
        kind: current + '_' + callbacks.filter((c) => c.root === current).length,
        root: current,
        captures,
    };
    callbacks.push(found);
    return found;
}
function expr(n) {
    if (ts.isParenthesizedExpression(n) || ts.isAsExpression(n) || ts.isNonNullExpression(n)) return expr(n.expression);
    if (n.kind === ts.SyntaxKind.ThisKeyword) return 'this_value.clone()';
    if (n.kind === ts.SyntaxKind.NullKeyword) return 'h.null_value()';
    if (n.kind === ts.SyntaxKind.TrueKeyword || n.kind === ts.SyntaxKind.FalseKeyword)
        return `h.boolean(${n.kind === ts.SyntaxKind.TrueKeyword})`;
    if (ts.isNumericLiteral(n))
        return `h.number(${/[.eE]/.test(Number(n.text).toString()) ? Number(n.text) : Number(n.text) + '.0'})`;
    if (ts.isStringLiteral(n) || ts.isNoSubstitutionTemplateLiteral(n)) return text(n.text);
    if (ts.isIdentifier(n)) {
        if (n.text === 'undefined') return 'h.undefined()';
        if (globals.has(n.text)) return `h.global(${quote(n.text)})?`;
        return `${n.text}.clone()`;
    }
    if (ts.isArrowFunction(n)) {
        const c = callbackMeta(n);
        return `h.position_callback(PositionCallback::${c.kind}([${c.captures.map((v) => v + '.clone()').join(',')}]))?`;
    }
    if (ts.isPropertyAccessExpression(n) || ts.isElementAccessExpression(n)) {
        const property = ts.isPropertyAccessExpression(n),
            get = property
                ? `h.get(&object, ${quote(n.name.text)})?`
                : `{ let key = ${expr(n.argumentExpression)}; h.index(&object, &key)? }`;
        // Optional-chain short circuit spans the entire chain, not just the ?. member.
        const chain = [];
        let p = n;
        while (ts.isPropertyAccessExpression(p) || ts.isElementAccessExpression(p)) {
            chain.unshift(p);
            p = p.expression;
        }
        if (chain.some((p) => p.questionDotToken)) {
            let body = `let mut object = ${expr(p)};`;
            for (const c of chain) {
                const read = ts.isPropertyAccessExpression(c)
                    ? `h.get(&object, ${quote(c.name.text)})?`
                    : `{ let key = ${expr(c.argumentExpression)}; h.index(&object,&key)? }`;
                body += `${c.questionDotToken ? "if h.is_nullish(&object) { break 'optional h.undefined(); }" : ''} object = ${read};`;
            }
            return `{'optional: {${body}object}}`;
        }
        return `{let object = ${expr(n.expression)}; ${get}}`;
    }
    if (ts.isCallExpression(n)) {
        let pre, receiver;
        if (ts.isPropertyAccessExpression(n.expression)) {
            receiver = 'receiver';
            pre = `let receiver = ${expr(n.expression.expression)}; ${n.expression.questionDotToken ? "if h.is_nullish(&receiver) {break 'optional_call h.undefined();}" : ''} let function = h.get(&receiver, ${quote(n.expression.name.text)})?;`;
        } else {
            receiver = 'receiver';
            pre = `let function = ${expr(n.expression)}; let receiver = h.undefined();`;
        }
        let a = n.arguments.map((x, i) => `let arg${i} = ${expr(x)};`).join('');
        return `{${ts.isPropertyAccessExpression(n.expression) && n.expression.questionDotToken ? "'optional_call: {" : ''}${pre}${a}h.call(&function,&${receiver},&[${n.arguments.map((_, i) => 'arg' + i).join(',')}])?${ts.isPropertyAccessExpression(n.expression) && n.expression.questionDotToken ? '}' : ''}}`;
    }
    if (ts.isNewExpression(n)) {
        return `{let function = ${expr(n.expression)};${(n.arguments ?? []).map((a, i) => `let arg${i} = ${expr(a)};`).join('')}h.construct(&function,&[${(n.arguments ?? []).map((_, i) => 'arg' + i).join(',')}])?}`;
    }
    if (ts.isAwaitExpression(n)) return `{let value = ${expr(n.expression)}; h.await_value(value).await?}`;
    if (ts.isConditionalExpression(n))
        return `{let condition = ${expr(n.condition)}; if h.truthy(&condition) {${expr(n.whenTrue)}} else {${expr(n.whenFalse)}}}`;
    if (ts.isPrefixUnaryExpression(n)) {
        const operand = expr(n.operand);
        if (n.operator === ts.SyntaxKind.ExclamationToken)
            return `{let value = ${operand}; h.boolean(!h.truthy(&value))}`;
        return `{let value = ${operand}; h.position_unary(${quote(n.operator === ts.SyntaxKind.PlusToken ? 'plus' : 'minus')}, &value)?}`;
    }
    if (ts.isBinaryExpression(n)) {
        const k = n.operatorToken.kind,
            a = expr(n.left),
            b = expr(n.right);
        if (
            [
                ts.SyntaxKind.BarBarToken,
                ts.SyntaxKind.AmpersandAmpersandToken,
                ts.SyntaxKind.QuestionQuestionToken,
            ].includes(k)
        )
            return `{let left = ${a}; if ${k === ts.SyntaxKind.QuestionQuestionToken ? '!h.is_nullish(&left)' : k === ts.SyntaxKind.BarBarToken ? 'h.truthy(&left)' : '!h.truthy(&left)'} {left} else {${b}}}`;
        if ([ts.SyntaxKind.EqualsEqualsEqualsToken, ts.SyntaxKind.ExclamationEqualsEqualsToken].includes(k))
            return `{let left = ${a}; let right = ${b}; h.boolean(${k === ts.SyntaxKind.ExclamationEqualsEqualsToken ? '!' : ''}h.strict_equal(&left,&right))}`;
        if ([ts.SyntaxKind.EqualsEqualsToken, ts.SyntaxKind.ExclamationEqualsToken].includes(k)) {
            if (n.right.kind !== ts.SyntaxKind.NullKeyword) throw new Error('non-null loose equality');
            return `{let left = ${a}; h.boolean(${k === ts.SyntaxKind.ExclamationEqualsToken ? '!' : ''}h.is_nullish(&left))}`;
        } // Only source != null; assert below.
        const ops = new Map([
            [ts.SyntaxKind.PlusToken, 'add'],
            [ts.SyntaxKind.MinusToken, 'subtract'],
            [ts.SyntaxKind.AsteriskToken, 'multiply'],
            [ts.SyntaxKind.SlashToken, 'divide'],
            [ts.SyntaxKind.LessThanToken, 'less'],
            [ts.SyntaxKind.LessThanEqualsToken, 'lessEqual'],
            [ts.SyntaxKind.GreaterThanToken, 'greater'],
            [ts.SyntaxKind.GreaterThanEqualsToken, 'greaterEqual'],
        ]);
        if (ops.has(k))
            return `{let left = ${a}; let right = ${b}; h.position_operator(${quote(ops.get(k))},&left,&right)?}`;
        throw new Error('binary ' + n.getText());
    }
    if (ts.isTemplateExpression(n)) {
        return `{let mut units: Vec<u16> = vec![${Array.from(n.head.text, (c) => c.charCodeAt(0)).join(',')}];${n.templateSpans.map((s) => `let value = ${expr(s.expression)}; units.extend(h.template_string(&value)?.0); units.extend_from_slice(&[${Array.from(s.literal.text, (c) => c.charCodeAt(0)).join(',')}]);`).join('')}h.literal(&JsString(units))}`;
    }
    if (ts.isObjectLiteralExpression(n)) {
        return `{let object = h.object()?;${n.properties
            .map((p) => {
                if (ts.isSpreadAssignment(p))
                    return `let value = ${expr(p.expression)}; h.spread_into(&object,&value)?;`;
                const name = ts.isStringLiteral(p.name) ? p.name.text : p.name.getText();
                const value = ts.isShorthandPropertyAssignment(p) ? p.name : p.initializer;
                return `let value = ${expr(value)}; h.define_data(&object,${quote(name)},&value)?;`;
            })
            .join('')}object}`;
    }
    if (ts.isArrayLiteralExpression(n)) {
        return `{let array = h.array()?;${n.elements.map((e, i) => (ts.isSpreadElement(e) ? `let value = ${expr(e.expression)}; h.append_position_spread(&array,&value)?;` : `let value = ${expr(e)}; h.array_define(&array,${i},&value)?;`)).join('')}array}`;
    }
    throw new Error('expression ' + ts.SyntaxKind[n.kind] + ' ' + n.getText());
}
function assign(n, value) {
    if (ts.isIdentifier(n)) return `${n.text} = ${value};`;
    if (ts.isPropertyAccessExpression(n))
        return `{let object = ${expr(n.expression)}; let value = ${value}; h.set(&object,${quote(n.name.text)},&value)?;}`;
    if (ts.isElementAccessExpression(n))
        return `{let object = ${expr(n.expression)};let key = ${expr(n.argumentExpression)};let value = ${value};h.set_position_index(&object,&key,&value)?;}`;
    throw new Error('assign');
}
function stmt(n) {
    if (ts.isBlock(n)) return `{${n.statements.map(stmt).join('\n')}}`;
    if (ts.isVariableStatement(n))
        return n.declarationList.declarations
            .map((d) => {
                if (ts.isIdentifier(d.name)) {
                    if (
                        current === 'handleOrderUpdates' &&
                        ['inferredType', 'inferredPrice', 'inferenceSource'].includes(d.name.text)
                    )
                        return `let ${d.name.text}:H::Value;`;
                    return `let ${mutated.has(d.name.text) ? 'mut ' : ''}${d.name.text} = ${d.initializer ? expr(d.initializer) : 'h.undefined()'};`;
                }
                if (ts.isObjectBindingPattern(d.name) && d.name.elements.length === 1) {
                    const v = d.name.elements[0].name.text;
                    return `let ${v} = {let object = ${expr(d.initializer)};h.get(&object,${quote(v)})?};`;
                }
                throw new Error('binding');
            })
            .join('\n');
    if (ts.isExpressionStatement(n)) {
        const x = n.expression;
        if (ts.isBinaryExpression(x) && x.operatorToken.kind === ts.SyntaxKind.EqualsToken)
            return assign(x.left, expr(x.right));
        if (ts.isBinaryExpression(x) && x.operatorToken.kind === ts.SyntaxKind.PlusEqualsToken)
            return assign(
                x.left,
                `{let left = ${expr(x.left)};let right = ${expr(x.right)};h.position_operator("add",&left,&right)?}`,
            );
        if (ts.isPostfixUnaryExpression(x) && ts.isIdentifier(x.operand)) return `${x.operand.text} += 1;`;
        return `let _ = ${expr(x)};`;
    }
    if (ts.isIfStatement(n))
        return `if {let condition = ${expr(n.expression)};h.truthy(&condition)} ${block(n.thenStatement)}${n.elseStatement ? ' else ' + block(n.elseStatement) : ''}`;
    if (ts.isReturnStatement(n)) return `return Ok(${n.expression ? expr(n.expression) : 'h.undefined()'});`;
    if (ts.isContinueStatement(n)) return 'return Ok(());';
    if (ts.isForOfStatement(n)) {
        const name = n.initializer.declarations[0].name.text;
        return `{let value = ${expr(n.expression)};let iterator = h.iterator(&value)?;while let Some(${name}) = h.next(&iterator)? {let step = (|| -> Result<(),H::Error> ${loopBlock(n.statement)})();if let Err(error) = step {h.close_after_throw(&iterator);return Err(error);}}}`;
    }
    if (ts.isForStatement(n)) {
        const d = n.initializer.declarations[0];
        const name = d.name.text;
        return `{let mut ${name}:usize = ${d.initializer.text};while {let left = h.number(${name} as f64);let right = ${expr(n.condition.right)};let condition = h.position_operator("less",&left,&right)?;h.truthy(&condition)} {let step = (|| -> Result<(),H::Error> ${loopBlock(n.statement, name)})();step?;${name} += 1;}}`;
    }
    if (ts.isTryStatement(n)) {
        const error = n.catchClause.variableDeclaration.name.text;
        return `{let outcome:Result<H::Value,H::Error> = ${current === 'fetchHip3Positions' ? 'async' : ''} {${n.tryBlock.statements.map(stmt).join('\n')}Ok(h.undefined())}${current === 'fetchHip3Positions' ? '.await' : '()'};if let Err(error) = outcome {let ${error} = h.error_value(&error);${n.catchClause.block.statements.map(stmt).join('\n')}}}`;
    }
    throw new Error('statement ' + ts.SyntaxKind[n.kind] + ' ' + n.getText());
}
function block(n) {
    return ts.isBlock(n) ? stmt(n) : `{${stmt(n)}}`;
}
function loopBlock(n, index) {
    let b = (ts.isBlock(n) ? n.statements : [n]).map(stmt).join('\n');
    if (index) b = b.replace(new RegExp(index + '\\.clone\\(\\)', 'g'), `h.number(${index} as f64)`);
    return `{${b}Ok(())}`;
}
function terminates(n) {
    return (
        ts.isReturnStatement(n) ||
        (ts.isBlock(n) && n.statements.length > 0 && terminates(n.statements.at(-1))) ||
        (ts.isIfStatement(n) && n.elseStatement && terminates(n.thenStatement) && terminates(n.elseStatement))
    );
}
function body(n) {
    if (!ts.isBlock(n)) return `return Ok(${expr(n)});`;
    const statements = n.statements.map(stmt);
    const last = n.statements.at(-1);
    return statements.join('\n') + (last && terminates(last) ? '' : '\nOk(h.undefined())');
}
const methods = roots.map(([owner, name]) => {
    current = name;
    const { ast, cls } = parsed[owner];
    const member = cls.members.find((m) => m.name?.getText(ast) === name);
    if (!member) throw new Error(name);
    const fn = member.initializer ?? member;
    mutated = collectMutated(fn.body);
    const parameters = fn.parameters.map((p) => p.name.text);
    const defaults = fn.parameters
        .filter((p) => p.initializer)
        .map(
            (p) =>
                `let ${p.name.text} = if h.strict_equal(&${p.name.text},&h.undefined()) {${expr(p.initializer)}} else {${p.name.text}};`,
        )
        .join('\n');
    const content = body(fn.body);
    const async = fn.modifiers?.some((m) => m.kind === ts.SyntaxKind.AsyncKeyword);
    return {
        owner,
        name,
        member,
        fn,
        ast,
        parameters,
        async,
        rust: `/// Source: ${owner === 'core' ? 'hyperliquid.exchange.ts' : 'hyperliquid.ws-private.ts'}::${name}\npub ${async ? 'async ' : ''}fn ${snake(name)}<H: PositionOrderHost>(h: &mut H, this_value: &H::Value${parameters.map((n) => `, ${n}: &H::Value`).join('')}) -> Result<H::Value,H::Error> {${parameters.map((p) => `let ${p} = ${p}.clone();`).join('\n')}${defaults}${content}}`,
    };
});
let cbRust = '';
for (const c of callbacks) {
    current = c.root;
    mutated = collectMutated(c.node.body);
    cbRust += `PositionCallback::${c.kind}(${c.captures.length ? 'captures' : '_'}) => {${c.captures.map((v, i) => `let ${v} = captures[${i}].clone();`).join('\n')}${c.node.parameters.map((p, i) => `let ${p.name.text} = args.get(${i}).cloned().unwrap_or_else(||h.undefined());`).join('\n')}${body(c.node.body)}}\n`;
}
const header = `//! Position and order ingestion bodies, lowered statement-by-statement from TypeScript.\n//! Virtual callees and ECMAScript primitives remain explicit venue-local host bindings.\nuse crate::{core_account_snapshots::CoreSnapshotHost, spot_json::JsString};\n#[derive(Clone,Debug)]\npub enum PositionCallback<V> {${callbacks.map((c) => `${c.kind}([V;${c.captures.length}]),`).join('\n')}}\npub trait PositionOrderHost: CoreSnapshotHost {\nfn position_operator(&mut self, operator:&str, left:&Self::Value,right:&Self::Value)->Result<Self::Value,Self::Error>;\nfn position_unary(&mut self, operator:&str, value:&Self::Value)->Result<Self::Value,Self::Error>;\nfn position_callback(&mut self, callback:PositionCallback<Self::Value>)->Result<Self::Value,Self::Error>;\nfn set_position_index(&mut self, object:&Self::Value,key:&Self::Value,value:&Self::Value)->Result<(),Self::Error>;\nfn append_position_spread(&mut self,array:&Self::Value,value:&Self::Value)->Result<(),Self::Error>;\n}\nfn lit<H:PositionOrderHost>(h:&H,s:&str)->H::Value {h.literal(&JsString(s.encode_utf16().collect()))}\npub fn run_position_callback<H:PositionOrderHost>(h:&mut H,callback:&PositionCallback<H::Value>,args:&[H::Value])->Result<H::Value,H::Error>{match callback {${cbRust}}}\n`;
// Name source-shaped bindings explicitly without broad lint exemptions.
// Rename only generated Rust identifiers, leaving literals/comments intact.
const renames = new Map();
for (const m of methods) {
    walk(m.fn.body, (n) => {
        if ((ts.isVariableDeclaration(n) || ts.isParameter(n)) && ts.isIdentifier(n.name) && /[A-Z]/.test(n.name.text))
            renames.set(n.name.text, snake(n.name.text));
    });
    for (const p of m.parameters) if (/[A-Z]/.test(p)) renames.set(p, snake(p));
}
for (const c of callbacks)
    renames.set(
        c.kind,
        c.kind
            .split('_')
            .map((x) => x[0].toUpperCase() + x.slice(1))
            .join(''),
    );
let output = header + methods.map((m) => m.rust).join('\n');
output = output.replace(/"(?:[^"\\]|\\.)*"|\b[A-Za-z][A-Za-z0-9_]*\b/g, (w) =>
    w.startsWith('"') ? w : (renames.get(w) ?? w),
);
if (require.main === module) fs.writeFileSync(path.resolve(__dirname, '../src/core_position_order_state.rs'), output);
module.exports = { ts, roots, globals, parsed, methods, callbacks, callbackMeta, snake };
