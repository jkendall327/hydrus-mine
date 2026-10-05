//! The locator's closed numeric expression language; no application or Python execution.
use num_bigint::BigInt;
use num_traits::{FromPrimitive as _, One as _, Signed as _, ToPrimitive as _, Zero as _};

#[derive(Clone, Debug)]
enum Number {
    Integer(BigInt),
    Float(f64),
}
impl Number {
    fn float(&self) -> Option<f64> {
        match self {
            Self::Integer(value) => value.to_f64().filter(|value| value.is_finite()),
            Self::Float(value) => Some(*value),
        }
    }
    fn negate(self) -> Self {
        match self {
            Self::Integer(value) => Self::Integer(-value),
            Self::Float(value) => Self::Float(-value),
        }
    }
    fn display(&self) -> Option<String> {
        match self {
            Self::Integer(value) => {
                let text = value.to_string();
                (text.trim_start_matches('-').len() <= 4300).then_some(text)
            }
            Self::Float(value) => {
                let mut text = String::new();
                hydrus_core::pyjson::write_python_float(*value, &mut text);
                Some(match text.as_str() {
                    "NaN" => "nan".into(),
                    "Infinity" => "inf".into(),
                    "-Infinity" => "-inf".into(),
                    _ => text,
                })
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Operator {
    Add,
    Subtract,
    Multiply,
    Divide,
    FloorDivide,
    Modulo,
    Power,
}
fn apply(left: &Number, op: Operator, right: &Number) -> Option<Number> {
    if let (Number::Integer(a), Number::Integer(b)) = (left, right) {
        let value = match op {
            Operator::Add => Some(a + b),
            Operator::Subtract => Some(a - b),
            Operator::Multiply => Some(a * b),
            Operator::FloorDivide | Operator::Modulo if !b.is_zero() => {
                let mut quotient = a / b;
                if !(a % b).is_zero() && a.is_negative() != b.is_negative() {
                    quotient -= 1;
                }
                Some(if matches!(op, Operator::Modulo) {
                    a - b * quotient
                } else {
                    quotient
                })
            }
            Operator::Power if !b.is_negative() => {
                // Output has the same 4300-digit bound as the reference Python interpreter.
                // Avoid allocating an enormous intermediate that can never be displayed.
                if a.is_zero() || a.is_one() {
                    Some(if a.is_zero() && b.is_zero() {
                        BigInt::one()
                    } else {
                        a.clone()
                    })
                } else if *a == BigInt::from(-1) {
                    Some(if (b % 2_u8).is_zero() {
                        BigInt::one()
                    } else {
                        -BigInt::one()
                    })
                } else {
                    let exponent = b.to_u32()?;
                    if a.bits()
                        .saturating_sub(1)
                        .saturating_mul(u64::from(exponent))
                        > 15_000
                    {
                        return None;
                    }
                    Some(a.pow(exponent))
                }
            }
            _ => None,
        };
        if let Some(value) = value {
            return Some(Number::Integer(value));
        }
    }
    let a = left.float()?;
    let b = right.float()?;
    let value = match op {
        Operator::Add => a + b,
        Operator::Subtract => a - b,
        Operator::Multiply => a * b,
        Operator::Divide if b != 0.0 => a / b,
        Operator::FloorDivide | Operator::Modulo if b != 0.0 => {
            let mut remainder = a % b;
            let mut quotient = (a - remainder) / b;
            if remainder == 0.0 {
                remainder = 0.0_f64.copysign(b);
            } else if remainder.is_sign_negative() != b.is_sign_negative() {
                remainder += b;
                quotient -= 1.0;
            }
            if matches!(op, Operator::Modulo) {
                remainder
            } else if quotient == 0.0 {
                0.0_f64.copysign(a / b)
            } else {
                let floor = quotient.floor();
                if quotient - floor > 0.5 {
                    floor + 1.0
                } else {
                    floor
                }
            }
        }
        Operator::Power => {
            if (a == 0.0 && b < 0.0)
                || (a < 0.0 && a.is_finite() && b.is_finite() && b.fract() != 0.0)
            {
                return None;
            }
            let value = a.powf(b);
            if value.is_infinite() && a.is_finite() && b.is_finite() {
                return None;
            }
            value
        }
        _ => return None,
    };
    Some(Number::Float(value))
}

fn function(name: &str, argument: Option<Number>) -> Option<Number> {
    if argument.is_none() {
        return match name {
            "gcd" => Some(Number::Integer(BigInt::zero())),
            "hypot" => Some(Number::Float(0.0)),
            "random" => Some(Number::Float(rand::random::<f64>())),
            _ => None,
        };
    }
    let number = argument?;
    match name {
        "abs" => {
            return Some(match number {
                Number::Integer(value) => Number::Integer(value.abs()),
                Number::Float(value) => Number::Float(value.abs()),
            });
        }
        "gcd" => {
            return match number {
                Number::Integer(value) => Some(Number::Integer(value.abs())),
                Number::Float(_) => None,
            };
        }
        "factorial" => {
            let Number::Integer(value) = number else {
                return None;
            };
            let count = value.to_u32()?;
            if count > 1558 {
                return None;
            }
            let result = (2..=count).fold(BigInt::one(), |result, i| result * i);
            return Some(Number::Integer(result));
        }
        "ceil" | "floor" => {
            if let Number::Integer(value) = number {
                return Some(Number::Integer(value));
            }
            let value = number.float()?;
            return BigInt::from_f64(if name == "ceil" {
                value.ceil()
            } else {
                value.floor()
            })
            .map(Number::Integer);
        }
        _ => {}
    }
    let value = number.float()?;
    let output = match name {
        "exp" => value.exp(),
        "log" if value > 0.0 || value.is_nan() => value.ln(),
        "log2" if value > 0.0 || value.is_nan() => value.log2(),
        "log10" if value > 0.0 || value.is_nan() => value.log10(),
        "sqrt" if value >= 0.0 || value.is_nan() => value.sqrt(),
        "acos" if value.abs() <= 1.0 || value.is_nan() => value.acos(),
        "asin" if value.abs() <= 1.0 || value.is_nan() => value.asin(),
        "atan" => value.atan(),
        "cos" if !value.is_infinite() => value.cos(),
        "sin" if !value.is_infinite() => value.sin(),
        "tan" if !value.is_infinite() => value.tan(),
        "hypot" => value.abs(),
        "degrees" => value * (180.0 / std::f64::consts::PI),
        "radians" => value * (std::f64::consts::PI / 180.0),
        "acosh" if value >= 1.0 || value.is_nan() => value.acosh(),
        "asinh" => value.asinh(),
        "atanh" if value.abs() < 1.0 || value.is_nan() => value.atanh(),
        "cosh" => value.cosh(),
        "sinh" => value.sinh(),
        "tanh" => value.tanh(),
        "erf" => libm::erf(value),
        "erfc" => libm::erfc(value),
        "gamma" | "lgamma" => {
            if value == 0.0
                || (value < 0.0 && value.fract() == 0.0)
                || (name == "gamma" && value == f64::NEG_INFINITY)
            {
                return None;
            }
            if name == "gamma" {
                libm::tgamma(value)
            } else {
                libm::lgamma(value)
            }
        }
        // Commas are excluded by the reference's safe pattern, so these cannot
        // receive the two arguments they require. Likewise random takes none.
        _ => return None,
    };
    if output.is_infinite() && value.is_finite() && !matches!(name, "degrees" | "radians") {
        return None;
    }
    Some(Number::Float(output))
}

#[derive(Debug)]
struct Parser<'a> {
    input: &'a [u8],
    position: usize,
    depth: usize,
}
impl Parser<'_> {
    fn space(&mut self) {
        while self
            .input
            .get(self.position)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.position += 1;
        }
    }
    fn take(&mut self, token: &[u8]) -> bool {
        self.space();
        if self.input[self.position..].starts_with(token) {
            self.position += token.len();
            true
        } else {
            false
        }
    }
    fn sum(&mut self) -> Option<Number> {
        let mut value = self.product()?;
        loop {
            let op = if self.take(b"+") {
                Operator::Add
            } else if self.take(b"-") {
                Operator::Subtract
            } else {
                return Some(value);
            };
            value = apply(&value, op, &self.product()?)?;
        }
    }
    fn product(&mut self) -> Option<Number> {
        let mut value = self.unary()?;
        loop {
            self.space();
            if self.input[self.position..].starts_with(b"**") {
                return Some(value);
            }
            let op = if self.take(b"//") {
                Operator::FloorDivide
            } else if self.take(b"*") {
                Operator::Multiply
            } else if self.take(b"/") {
                Operator::Divide
            } else if self.take(b"%") {
                Operator::Modulo
            } else {
                return Some(value);
            };
            value = apply(&value, op, &self.unary()?)?;
        }
    }
    fn unary(&mut self) -> Option<Number> {
        self.depth += 1;
        if self.depth > 200 {
            return None;
        }
        let value = if self.take(b"+") {
            self.unary()
        } else if self.take(b"-") {
            self.unary().map(Number::negate)
        } else {
            let value = self.atom()?;
            if self.take(b"**") {
                apply(&value, Operator::Power, &self.unary()?)
            } else {
                Some(value)
            }
        };
        self.depth -= 1;
        value
    }
    fn atom(&mut self) -> Option<Number> {
        if self.take(b"(") {
            let value = self.sum()?;
            return self.take(b")").then_some(value);
        }
        self.space();
        let start = self.position;
        if self.input.get(start).is_some_and(u8::is_ascii_lowercase) {
            while self
                .input
                .get(self.position)
                .is_some_and(u8::is_ascii_lowercase)
            {
                self.position += 1;
            }
            // log2/log10/atan2 are the only allowed names containing digits.
            while self
                .input
                .get(self.position)
                .is_some_and(u8::is_ascii_digit)
            {
                self.position += 1;
            }
            let name = std::str::from_utf8(&self.input[start..self.position]).ok()?;
            if self.take(b"(") {
                let argument = if self.take(b")") {
                    None
                } else {
                    let value = self.sum()?;
                    if !self.take(b")") {
                        return None;
                    }
                    Some(value)
                };
                return function(name, argument);
            }
            return match name {
                "pi" => Some(Number::Float(std::f64::consts::PI)),
                "e" => Some(Number::Float(std::f64::consts::E)),
                "inf" => Some(Number::Float(f64::INFINITY)),
                _ => None,
            };
        }
        self.literal()
    }
    fn digits(&mut self) {
        while self
            .input
            .get(self.position)
            .is_some_and(u8::is_ascii_digit)
        {
            self.position += 1;
        }
    }
    fn literal(&mut self) -> Option<Number> {
        let start = self.position;
        self.digits();
        let mut floating = false;
        if self.input.get(self.position) == Some(&b'.') {
            floating = true;
            self.position += 1;
            self.digits();
        }
        if self.input.get(self.position) == Some(&b'e') {
            floating = true;
            self.position += 1;
            if self
                .input
                .get(self.position)
                .is_some_and(|c| matches!(c, b'+' | b'-'))
            {
                self.position += 1;
            }
            let digits = self.position;
            self.digits();
            if digits == self.position {
                return None;
            }
        }
        let text = std::str::from_utf8(&self.input[start..self.position]).ok()?;
        if floating {
            text.parse().ok().map(Number::Float)
        } else {
            if text.len() > 4300 || (text.starts_with('0') && text.bytes().any(|b| b != b'0')) {
                return None;
            }
            text.parse().ok().map(Number::Integer)
        }
    }
}

pub(crate) fn calculate(text: &str) -> Option<String> {
    // Python permits Unicode whitespace in the safe pattern, but only ordinary
    // ASCII whitespace in numeric expressions (non-breaking spaces are SyntaxError).
    if !text.is_ascii() || text.trim().is_empty() {
        return None;
    }
    let mut parser = Parser {
        input: text.trim().as_bytes(),
        position: 0,
        depth: 0,
    };
    let value = parser.sum()?;
    parser.space();
    if parser.position != parser.input.len() {
        return None;
    }
    value.display()
}
