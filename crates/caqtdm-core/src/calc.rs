//! EPICS/MEDM calc expressions used by caCalc, caGraphics visibility, and caImage.
//!
//! The numeric dialect is the one the EPICS calc record and MEDM share:
//! variables `A` through `L`, C-like operators, and the usual math functions.
//! caQtDM also accepts a regular expression of the form `%/pattern/` which is
//! matched against the string value of channel A.

use std::fmt;

/// Inputs for one evaluation. Unset variables read as 0, matching a calc
/// record before its inputs connect.
#[derive(Debug, Clone)]
pub struct CalcInputs {
    pub vars: [f64; 12],
    pub strings: [String; 12],
}

impl Default for CalcInputs {
    fn default() -> Self {
        Self {
            vars: [0.0; 12],
            strings: std::array::from_fn(|_| String::new()),
        }
    }
}

impl CalcInputs {
    pub fn set(&mut self, index: usize, value: f64) {
        if let Some(slot) = self.vars.get_mut(index) {
            *slot = value;
        }
    }

    pub fn set_string(&mut self, index: usize, value: impl Into<String>) {
        if let Some(slot) = self.strings.get_mut(index) {
            *slot = value.into();
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CalcError {
    Empty,
    Syntax(String),
    Unknown(String),
    DivByZero,
    Arity(String),
    Unsupported(String),
}

impl fmt::Display for CalcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "empty calc expression"),
            Self::Syntax(m) => write!(f, "calc syntax: {m}"),
            Self::Unknown(m) => write!(f, "unknown calc symbol {m}"),
            Self::DivByZero => write!(f, "division by zero"),
            Self::Arity(m) => write!(f, "wrong number of arguments for {m}"),
            Self::Unsupported(m) => write!(f, "unsupported calc form: {m}"),
        }
    }
}

impl std::error::Error for CalcError {}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    Op(Op),
    LParen,
    RParen,
    Comma,
    Question,
    Colon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Or,
    And,
    BitOr,
    BitXor,
    BitAnd,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Shl,
    Shr,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Not,
    BitNot,
}

/// Evaluate a numeric calc expression.
pub fn eval_calc(expr: &str, inputs: &CalcInputs) -> Result<f64, CalcError> {
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        return Err(CalcError::Empty);
    }
    if trimmed.starts_with("%P/") || trimmed.starts_with("%p/") {
        return eval_python(trimmed, inputs);
    }
    let tokens = tokenize(trimmed)?;
    let mut parser = Parser {
        tokens: &tokens,
        pos: 0,
    };
    let value = parser.parse_ternary(inputs)?;
    if parser.pos != tokens.len() {
        return Err(CalcError::Syntax("trailing input".to_string()));
    }
    Ok(value)
}

/// Visibility expression. A `%/regex/` form matches channel A's string.
/// Anything else is a numeric calc, and the widget is visible when the
/// result is non-zero.
pub fn eval_visibility(expr: &str, inputs: &CalcInputs) -> Result<bool, CalcError> {
    let trimmed = expr.trim();
    if let Some(pattern) = regex_body(trimmed) {
        let text = inputs.strings.first().map(String::as_str).unwrap_or("");
        let re = compile_simple_regex(pattern)?;
        return Ok(re.is_match(text));
    }
    Ok(eval_calc(trimmed, inputs)? != 0.0)
}

fn eval_python(expr: &str, inputs: &CalcInputs) -> Result<f64, CalcError> {
    #[cfg(not(feature = "python"))]
    {
        let _ = (expr, inputs);
        Err(CalcError::Unsupported(
            "Python visibility expressions need the python feature".to_string(),
        ))
    }
    #[cfg(feature = "python")]
    {
        let body = python_body(expr)?;
        match eval_python_embedded(body, inputs) {
            Ok(value) => Ok(value),
            Err(_) => eval_python_shell(body, inputs),
        }
    }
}

#[cfg(feature = "python")]
fn eval_python_embedded(body: &str, inputs: &CalcInputs) -> Result<f64, CalcError> {
    let embedded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pyo3::Python::attach(|py| embedded_call(py, body, inputs))
    }));
    match embedded {
        Ok(value) => value,
        Err(_) => Err(CalcError::Unsupported("embedded python failed".into())),
    }
}

#[cfg(feature = "python")]
fn embedded_call(py: pyo3::Python<'_>, body: &str, inputs: &CalcInputs) -> Result<f64, CalcError> {
    use pyo3::prelude::*;
    use pyo3::types::PyDict;
    let code = std::ffi::CString::new(body)
        .map_err(|_| CalcError::Unsupported("python source contains a nul".into()))?;
    let text = std::ffi::CStr::from_bytes_with_nul(code.as_bytes_with_nul())
        .map_err(|err| CalcError::Unsupported(err.to_string()))?;
    let locals = PyDict::new(py);
    py.run(text, None, Some(&locals))
        .map_err(|err| CalcError::Unsupported(err.to_string()))?;
    let func = locals
        .get_item("PythonCalc")
        .map_err(|err| CalcError::Unsupported(err.to_string()))?
        .ok_or_else(|| CalcError::Unsupported("PythonCalc missing".into()))?;
    let value = func
        .call1((
            inputs.vars[0],
            inputs.vars[1],
            inputs.vars[2],
            inputs.vars[3],
        ))
        .map_err(|err| CalcError::Unsupported(err.to_string()))?;
    value
        .extract::<f64>()
        .map_err(|err: pyo3::PyErr| CalcError::Unsupported(err.to_string()))
}

#[cfg(feature = "python")]
fn eval_python_shell(body: &str, inputs: &CalcInputs) -> Result<f64, CalcError> {
    let mut child = std::process::Command::new("python3")
        .arg("-c")
        .arg(PYTHON_RUNNER)
        .args(inputs.vars.iter().take(4).map(|v| v.to_string()))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|err| CalcError::Unsupported(format!("python3: {err}")))?;
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin
            .write_all(body.as_bytes())
            .map_err(|err| CalcError::Unsupported(format!("python3 stdin: {err}")))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|err| CalcError::Unsupported(format!("python3: {err}")))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(CalcError::Unsupported(format!(
            "PythonCalc failed: {}",
            err.trim()
        )));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.trim()
        .parse::<f64>()
        .map_err(|_| CalcError::Unsupported(format!("PythonCalc returned {text:?}")))
}

#[cfg(feature = "python")]
fn python_body(expr: &str) -> Result<&str, CalcError> {
    let rest = expr
        .get(3..)
        .ok_or_else(|| CalcError::Syntax("python form".into()))?;
    let body = rest.strip_suffix('/').unwrap_or(rest).trim();
    if !body.contains("PythonCalc") {
        return Err(CalcError::Unsupported(
            "python function not found, must be called PythonCalc".into(),
        ));
    }
    Ok(body)
}

#[cfg(feature = "python")]
const PYTHON_RUNNER: &str = r#"
import sys
source = sys.stdin.read()
namespace = {"__builtins__": __builtins__}
exec(source, namespace)
fn = namespace.get("PythonCalc")
if not callable(fn):
    sys.stderr.write("PythonCalc missing\n")
    sys.exit(2)
args = [float(item) for item in sys.argv[1:5]]
print(float(fn(*args)))
"#;

fn regex_body(expr: &str) -> Option<&str> {
    let rest = expr.strip_prefix("%/")?;
    let end = rest.rfind('/')?;
    Some(&rest[..end])
}

fn compile_simple_regex(pattern: &str) -> Result<regex::Regex, CalcError> {
    regex::Regex::new(pattern).map_err(|err| CalcError::Syntax(err.to_string()))
}

struct Parser<'a> {
    tokens: &'a [Tok],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.pos)
    }

    fn bump(&mut self) -> Option<&Tok> {
        let t = self.tokens.get(self.pos);
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn parse_ternary(&mut self, inputs: &CalcInputs) -> Result<f64, CalcError> {
        let cond = self.parse_or(inputs)?;
        if matches!(self.peek(), Some(Tok::Question)) {
            self.bump();
            let yes = self.parse_ternary(inputs)?;
            if !matches!(self.bump(), Some(Tok::Colon)) {
                return Err(CalcError::Syntax("expected ':' in ternary".to_string()));
            }
            let no = self.parse_ternary(inputs)?;
            return Ok(if cond != 0.0 { yes } else { no });
        }
        Ok(cond)
    }

    fn parse_or(&mut self, inputs: &CalcInputs) -> Result<f64, CalcError> {
        let mut v = self.parse_and(inputs)?;
        while matches!(self.peek(), Some(Tok::Op(Op::Or))) {
            self.bump();
            let r = self.parse_and(inputs)?;
            v = if v != 0.0 || r != 0.0 { 1.0 } else { 0.0 };
        }
        Ok(v)
    }

    fn parse_and(&mut self, inputs: &CalcInputs) -> Result<f64, CalcError> {
        let mut v = self.parse_bitor(inputs)?;
        while matches!(self.peek(), Some(Tok::Op(Op::And))) {
            self.bump();
            let r = self.parse_bitor(inputs)?;
            v = if v != 0.0 && r != 0.0 { 1.0 } else { 0.0 };
        }
        Ok(v)
    }

    fn parse_bitor(&mut self, inputs: &CalcInputs) -> Result<f64, CalcError> {
        self.bin(inputs, Self::parse_bitxor, |t| {
            matches!(t, Tok::Op(Op::BitOr))
        })
    }

    fn parse_bitxor(&mut self, inputs: &CalcInputs) -> Result<f64, CalcError> {
        self.bin(inputs, Self::parse_bitand, |t| {
            matches!(t, Tok::Op(Op::BitXor))
        })
    }

    fn parse_bitand(&mut self, inputs: &CalcInputs) -> Result<f64, CalcError> {
        self.bin(inputs, Self::parse_eq, |t| matches!(t, Tok::Op(Op::BitAnd)))
    }

    fn parse_eq(&mut self, inputs: &CalcInputs) -> Result<f64, CalcError> {
        let mut v = self.parse_rel(inputs)?;
        loop {
            match self.peek() {
                Some(Tok::Op(Op::Eq)) => {
                    self.bump();
                    let r = self.parse_rel(inputs)?;
                    v = if v == r { 1.0 } else { 0.0 };
                }
                Some(Tok::Op(Op::Ne)) => {
                    self.bump();
                    let r = self.parse_rel(inputs)?;
                    v = if v != r { 1.0 } else { 0.0 };
                }
                _ => break,
            }
        }
        Ok(v)
    }

    fn parse_rel(&mut self, inputs: &CalcInputs) -> Result<f64, CalcError> {
        let mut v = self.parse_shift(inputs)?;
        while let Some(Tok::Op(op @ (Op::Lt | Op::Le | Op::Gt | Op::Ge))) = self.peek().cloned() {
            self.bump();
            let r = self.parse_shift(inputs)?;
            let bit = match op {
                Op::Lt => v < r,
                Op::Le => v <= r,
                Op::Gt => v > r,
                Op::Ge => v >= r,
                _ => false,
            };
            v = if bit { 1.0 } else { 0.0 };
        }
        Ok(v)
    }

    fn parse_shift(&mut self, inputs: &CalcInputs) -> Result<f64, CalcError> {
        let mut v = self.parse_add(inputs)?;
        while let Some(Tok::Op(op @ (Op::Shl | Op::Shr))) = self.peek().cloned() {
            self.bump();
            let r = self.parse_add(inputs)?;
            let shift = r as u32;
            v = match op {
                Op::Shl => (v as i64).wrapping_shl(shift) as f64,
                Op::Shr => (v as i64).wrapping_shr(shift) as f64,
                _ => v,
            };
        }
        Ok(v)
    }

    fn parse_add(&mut self, inputs: &CalcInputs) -> Result<f64, CalcError> {
        let mut v = self.parse_mul(inputs)?;
        loop {
            match self.peek() {
                Some(Tok::Op(Op::Add)) => {
                    self.bump();
                    v += self.parse_mul(inputs)?;
                }
                Some(Tok::Op(Op::Sub)) => {
                    self.bump();
                    v -= self.parse_mul(inputs)?;
                }
                _ => break,
            }
        }
        Ok(v)
    }

    fn parse_mul(&mut self, inputs: &CalcInputs) -> Result<f64, CalcError> {
        let mut v = self.parse_unary(inputs)?;
        loop {
            match self.peek() {
                Some(Tok::Op(Op::Mul)) => {
                    self.bump();
                    v *= self.parse_unary(inputs)?;
                }
                Some(Tok::Op(Op::Div)) => {
                    self.bump();
                    let r = self.parse_unary(inputs)?;
                    if r == 0.0 {
                        return Err(CalcError::DivByZero);
                    }
                    v /= r;
                }
                Some(Tok::Op(Op::Rem)) => {
                    self.bump();
                    let r = self.parse_unary(inputs)?;
                    if r == 0.0 {
                        return Err(CalcError::DivByZero);
                    }
                    v %= r;
                }
                _ => break,
            }
        }
        Ok(v)
    }

    fn parse_unary(&mut self, inputs: &CalcInputs) -> Result<f64, CalcError> {
        match self.peek() {
            Some(Tok::Op(Op::Add)) => {
                self.bump();
                self.parse_unary(inputs)
            }
            Some(Tok::Op(Op::Sub)) => {
                self.bump();
                Ok(-self.parse_unary(inputs)?)
            }
            Some(Tok::Op(Op::Not)) => {
                self.bump();
                Ok(if self.parse_unary(inputs)? == 0.0 {
                    1.0
                } else {
                    0.0
                })
            }
            Some(Tok::Op(Op::BitNot)) => {
                self.bump();
                Ok((!(self.parse_unary(inputs)? as i64)) as f64)
            }
            _ => self.parse_primary(inputs),
        }
    }

    fn parse_primary(&mut self, inputs: &CalcInputs) -> Result<f64, CalcError> {
        match self.bump().cloned() {
            Some(Tok::Num(n)) => Ok(n),
            Some(Tok::Ident(name)) => {
                if matches!(self.peek(), Some(Tok::LParen)) {
                    self.bump();
                    let mut args = Vec::new();
                    if !matches!(self.peek(), Some(Tok::RParen)) {
                        loop {
                            args.push(self.parse_ternary(inputs)?);
                            if matches!(self.peek(), Some(Tok::Comma)) {
                                self.bump();
                                continue;
                            }
                            break;
                        }
                    }
                    if !matches!(self.bump(), Some(Tok::RParen)) {
                        return Err(CalcError::Syntax(format!("expected ')' after {name}")));
                    }
                    call_fn(&name, &args)
                } else {
                    ident_value(&name, inputs)
                }
            }
            Some(Tok::LParen) => {
                let v = self.parse_ternary(inputs)?;
                if !matches!(self.bump(), Some(Tok::RParen)) {
                    return Err(CalcError::Syntax("expected ')'".to_string()));
                }
                Ok(v)
            }
            other => Err(CalcError::Syntax(format!("unexpected {other:?}"))),
        }
    }

    fn bin(
        &mut self,
        inputs: &CalcInputs,
        next: fn(&mut Self, &CalcInputs) -> Result<f64, CalcError>,
        is_op: fn(&Tok) -> bool,
    ) -> Result<f64, CalcError> {
        let mut v = next(self, inputs)?;
        while self.peek().is_some_and(is_op) {
            let op = match self.bump() {
                Some(Tok::Op(op)) => *op,
                _ => break,
            };
            let r = next(self, inputs)?;
            v = match op {
                Op::BitOr => ((v as i64) | (r as i64)) as f64,
                Op::BitXor => ((v as i64) ^ (r as i64)) as f64,
                Op::BitAnd => ((v as i64) & (r as i64)) as f64,
                _ => v,
            };
        }
        Ok(v)
    }
}

fn ident_value(name: &str, inputs: &CalcInputs) -> Result<f64, CalcError> {
    let upper = name.to_ascii_uppercase();
    match upper.as_str() {
        "A" | "B" | "C" | "D" | "E" | "F" | "G" | "H" | "I" | "J" | "K" | "L" => {
            let idx = (upper.as_bytes()[0] - b'A') as usize;
            Ok(inputs.vars[idx])
        }
        "VAL" => Ok(inputs.vars[0]),
        "PI" => Ok(std::f64::consts::PI),
        "D2R" => Ok(std::f64::consts::PI / 180.0),
        "R2D" => Ok(180.0 / std::f64::consts::PI),
        _ => Err(CalcError::Unknown(name.to_string())),
    }
}

fn call_fn(name: &str, args: &[f64]) -> Result<f64, CalcError> {
    let n = name.to_ascii_uppercase();
    let one = || {
        if args.len() != 1 {
            Err(CalcError::Arity(n.clone()))
        } else {
            Ok(args[0])
        }
    };
    let two = || {
        if args.len() != 2 {
            Err(CalcError::Arity(n.clone()))
        } else {
            Ok((args[0], args[1]))
        }
    };
    match n.as_str() {
        "ABS" => Ok(one()?.abs()),
        "SQR" => {
            let v = one()?;
            Ok(v * v)
        }
        "SQRT" => Ok(one()?.sqrt()),
        "CEIL" => Ok(one()?.ceil()),
        "FLOOR" => Ok(one()?.floor()),
        "NINT" => Ok(one()?.round()),
        "LOG" | "LOG10" => Ok(one()?.log10()),
        "LOGE" | "LN" => Ok(one()?.ln()),
        "EXP" => Ok(one()?.exp()),
        "SIN" => Ok(one()?.sin()),
        "COS" => Ok(one()?.cos()),
        "TAN" => Ok(one()?.tan()),
        "ASIN" => Ok(one()?.asin()),
        "ACOS" => Ok(one()?.acos()),
        "ATAN" => Ok(one()?.atan()),
        "SINH" => Ok(one()?.sinh()),
        "COSH" => Ok(one()?.cosh()),
        "TANH" => Ok(one()?.tanh()),
        "SGN" => {
            let v = one()?;
            Ok(if v > 0.0 {
                1.0
            } else if v < 0.0 {
                -1.0
            } else {
                0.0
            })
        }
        "ATAN2" => {
            let (y, x) = two()?;
            Ok(y.atan2(x))
        }
        "FMOD" => {
            let (a, b) = two()?;
            if b == 0.0 {
                return Err(CalcError::DivByZero);
            }
            Ok(a % b)
        }
        "MIN" => {
            if args.is_empty() {
                return Err(CalcError::Arity(n));
            }
            Ok(args.iter().copied().fold(f64::INFINITY, f64::min))
        }
        "MAX" => {
            if args.is_empty() {
                return Err(CalcError::Arity(n));
            }
            Ok(args.iter().copied().fold(f64::NEG_INFINITY, f64::max))
        }
        "RNDM" => Ok(simple_rand(args.first().copied().unwrap_or(1.0))),
        "FINITE" | "ISFINITE" => Ok(if one()?.is_finite() { 1.0 } else { 0.0 }),
        "ISNAN" => Ok(if one()?.is_nan() { 1.0 } else { 0.0 }),
        "ISINF" => Ok(if one()?.is_infinite() { 1.0 } else { 0.0 }),
        _ => Err(CalcError::Unknown(n)),
    }
}

fn simple_rand(seed: f64) -> f64 {
    let bits = seed.to_bits().wrapping_mul(0x9E37_79B9_7F4A_7C15);
    (bits >> 11) as f64 / ((1u64 << 53) as f64)
}

fn tokenize(expr: &str) -> Result<Vec<Tok>, CalcError> {
    let b = expr.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if c == b'(' {
            out.push(Tok::LParen);
            i += 1;
            continue;
        }
        if c == b')' {
            out.push(Tok::RParen);
            i += 1;
            continue;
        }
        if c == b',' {
            out.push(Tok::Comma);
            i += 1;
            continue;
        }
        if c == b'?' {
            out.push(Tok::Question);
            i += 1;
            continue;
        }
        if c == b':' {
            out.push(Tok::Colon);
            i += 1;
            continue;
        }
        if let Some((op, len)) = match_op(&b[i..]) {
            out.push(Tok::Op(op));
            i += len;
            continue;
        }
        if c.is_ascii_digit() || (c == b'.' && b.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let start = i;
            i += 1;
            while i < b.len()
                && (b[i].is_ascii_digit() || b[i] == b'.' || b[i] == b'e' || b[i] == b'E')
            {
                if (b[i] == b'e' || b[i] == b'E')
                    && b.get(i + 1).is_some_and(|s| *s == b'+' || *s == b'-')
                {
                    i += 2;
                    continue;
                }
                i += 1;
            }
            let text = &expr[start..i];
            let n: f64 = text
                .parse()
                .map_err(|_| CalcError::Syntax(format!("bad number {text}")))?;
            out.push(Tok::Num(n));
            continue;
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            i += 1;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            out.push(Tok::Ident(expr[start..i].to_string()));
            continue;
        }
        return Err(CalcError::Syntax(format!(
            "unexpected character '{}'",
            c as char
        )));
    }
    Ok(out)
}

fn match_op(s: &[u8]) -> Option<(Op, usize)> {
    let two = |a, b| s.len() >= 2 && s[0] == a && s[1] == b;
    if two(b'|', b'|') {
        Some((Op::Or, 2))
    } else if two(b'&', b'&') {
        Some((Op::And, 2))
    } else if two(b'=', b'=') {
        Some((Op::Eq, 2))
    } else if two(b'!', b'=') {
        Some((Op::Ne, 2))
    } else if two(b'<', b'=') {
        Some((Op::Le, 2))
    } else if two(b'>', b'=') {
        Some((Op::Ge, 2))
    } else if two(b'<', b'<') {
        Some((Op::Shl, 2))
    } else if two(b'>', b'>') {
        Some((Op::Shr, 2))
    } else if s[0] == b'|' {
        Some((Op::BitOr, 1))
    } else if s[0] == b'^' {
        Some((Op::BitXor, 1))
    } else if s[0] == b'&' {
        Some((Op::BitAnd, 1))
    } else if s[0] == b'<' {
        Some((Op::Lt, 1))
    } else if s[0] == b'>' {
        Some((Op::Gt, 1))
    } else if s[0] == b'+' {
        Some((Op::Add, 1))
    } else if s[0] == b'-' {
        Some((Op::Sub, 1))
    } else if s[0] == b'*' {
        Some((Op::Mul, 1))
    } else if s[0] == b'/' {
        Some((Op::Div, 1))
    } else if s[0] == b'%' {
        Some((Op::Rem, 1))
    } else if s[0] == b'!' {
        Some((Op::Not, 1))
    } else if s[0] == b'~' {
        Some((Op::BitNot, 1))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_a(a: f64) -> CalcInputs {
        let mut i = CalcInputs::default();
        i.set(0, a);
        i
    }

    #[test]
    fn arithmetic_and_functions() {
        let mut inputs = CalcInputs::default();
        inputs.set(0, 2.0);
        inputs.set(1, 3.0);
        inputs.set(2, 4.0);
        assert_eq!(eval_calc("A+1", &inputs).unwrap(), 3.0);
        assert_eq!(eval_calc("(A+B)*C", &inputs).unwrap(), 20.0);
        assert_eq!(eval_calc("ABS(A-B)", &inputs).unwrap(), 1.0);
        assert_eq!(eval_calc("SQR(A)", &inputs).unwrap(), 4.0);
        assert_eq!(eval_calc("SQRT(9)", &inputs).unwrap(), 3.0);
        assert_eq!(eval_calc("MAX(A,B,C)", &inputs).unwrap(), 4.0);
        assert_eq!(eval_calc("MIN(A,B)", &inputs).unwrap(), 2.0);
        assert!((eval_calc("LOG(100)", &inputs).unwrap() - 2.0).abs() < 1e-9);
        assert!((eval_calc("LOGE(1)", &inputs).unwrap()).abs() < 1e-12);
        assert_eq!(eval_calc("2<<3", &inputs).unwrap(), 16.0);
        assert_eq!(eval_calc("NINT(1.6)", &inputs).unwrap(), 2.0);
        assert_eq!(eval_calc("A>5?1:0", &with_a(6.0)).unwrap(), 1.0);
        assert_eq!(eval_calc("A>5?1:0", &with_a(4.0)).unwrap(), 0.0);
        assert!(eval_calc("1/0", &inputs).is_err());
    }

    #[test]
    fn visibility_regex() {
        let mut inputs = CalcInputs::default();
        inputs.set_string(0, "ON");
        assert!(eval_visibility("%/^ON$/", &inputs).unwrap());
        assert!(!eval_visibility("%/^OFF$/", &inputs).unwrap());
        inputs.set(0, 3.0);
        assert!(eval_visibility("A", &inputs).unwrap());
        inputs.set(0, 0.0);
        assert!(!eval_visibility("A", &inputs).unwrap());
    }

    #[cfg(feature = "python")]
    #[test]
    fn python_calc_is_visible_when_the_function_is_nonzero() {
        let expr = "%P/def PythonCalc(a, b, c, d):\n    return a * b + c\n/";
        let mut inputs = CalcInputs::default();
        inputs.set(0, 2.0);
        inputs.set(1, 3.0);
        inputs.set(2, 1.0);
        assert_eq!(eval_calc(expr, &inputs).unwrap(), 7.0);
        assert!(eval_visibility(expr, &inputs).unwrap());
        inputs.set(0, 0.0);
        inputs.set(1, 0.0);
        inputs.set(2, 0.0);
        assert!(!eval_visibility(expr, &inputs).unwrap());
        assert!(eval_calc("%P/def Other(a, b, c, d):\n    return 1\n/", &inputs).is_err());
        inputs.set(0, 2.0);
        inputs.set(1, 3.0);
        inputs.set(2, 1.0);
        let body = python_body(expr).unwrap();
        assert_eq!(eval_python_embedded(body, &inputs).unwrap(), 7.0);
        assert_eq!(eval_python_shell(body, &inputs).unwrap(), 7.0);
    }
}
