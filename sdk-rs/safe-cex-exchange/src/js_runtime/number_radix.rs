// Copyright 2011 the V8 project authors. All rights reserved.
// Copyright 2014, the V8 project authors. All rights reserved.
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are
// met:
//
//     * Redistributions of source code must retain the above copyright
//       notice, this list of conditions and the following disclaimer.
//     * Redistributions in binary form must reproduce the above
//       copyright notice, this list of conditions and the following
//       disclaimer in the documentation and/or other materials provided
//       with the distribution.
//     * Neither the name of Google Inc. nor the names of its
//       contributors may be used to endorse or promote products derived
//       from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
// "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
// LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
// A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
// OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
// LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
// DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
// THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
// (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
// OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
//! Number radix conversion adapted from V8 DoubleToRadixStringView in Node
//! v24.20.0. See THIRD_PARTY_NOTICES.md. Rust vectors replace its split buffer.

/// The caller has applied ToIntegerOrInfinity and validated radix in 2..=36.
pub(super) fn format(value: f64, radix: u32) -> String {
    if radix == 10 || value == 0.0 || !value.is_finite() {
        return safe_cex_num::js_number_string(value);
    }
    let negative = value < 0.0;
    let value = value.abs();
    let base = f64::from(radix);
    let mut integer = value.floor();
    let mut fraction = value - integer;
    let next = f64::from_bits(value.to_bits() + 1);
    let mut delta = 0.5 * (next - value);
    if delta <= 0.0 {
        delta = f64::from_bits(1);
    }
    let mut fractional_digits = Vec::new();
    if fraction >= delta {
        loop {
            fraction *= base;
            delta *= base;
            let digit = fraction as u32;
            fractional_digits.push(digit);
            fraction -= f64::from(digit);
            if (fraction > 0.5 || fraction == 0.5 && digit & 1 == 1) && fraction + delta > 1.0 {
                // Carry through the fractional digits, removing rounded tails.
                loop {
                    match fractional_digits.pop() {
                        Some(digit) if digit + 1 < radix => {
                            fractional_digits.push(digit + 1);
                            break;
                        }
                        Some(_) => {}
                        None => {
                            integer += 1.0;
                            break;
                        }
                    }
                }
                break;
            }
            if fraction < delta {
                break;
            }
        }
    }
    let mut integer_digits = Vec::new();
    // V8 Double::Exponent subtracts the 52 stored significand bits as well as
    // the exponent bias. A positive exponent means low digits are unrepresented.
    while integer / base >= 9_007_199_254_740_992.0 {
        integer /= base;
        integer_digits.push(0);
    }
    loop {
        let remainder = integer % base;
        integer_digits.push(remainder as u32);
        integer = (integer - remainder) / base;
        if integer <= 0.0 {
            break;
        }
    }
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = String::with_capacity(integer_digits.len() + fractional_digits.len() + 2);
    if negative {
        out.push('-');
    }
    for digit in integer_digits.into_iter().rev() {
        out.push(char::from(DIGITS[digit as usize]));
    }
    if !fractional_digits.is_empty() {
        out.push('.');
        for digit in fractional_digits {
            out.push(char::from(DIGITS[digit as usize]));
        }
    }
    out
}
