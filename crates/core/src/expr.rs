use crate::vars::{Scope, VarError};
use base64::Engine;

pub const SUPPORTED: &[&str] = &["md5", "sha1", "sha256", "base64", "now"];

#[derive(Debug, thiserror::Error)]
pub enum ExprError {
    #[error("unknown function `{name}` — supported: {supported}")]
    UnknownFunction { name: String, supported: String },
    #[error("expression syntax error: {message}")]
    Syntax { message: String },
    #[error("`{name}` takes {expected} argument(s), found {found}")]
    Arity { name: String, expected: usize, found: usize },
    #[error(transparent)]
    Var(#[from] VarError),
}

pub fn eval(input: &str, scope: &Scope) -> Result<String, ExprError> {
    let mut p = Parser { s: input.as_bytes(), i: 0, scope };
    let value = p.expr()?;
    p.skip_ws();
    if p.i != p.s.len() {
        return Err(ExprError::Syntax { message: format!("unexpected trailing input at byte {}", p.i) });
    }
    Ok(value)
}

struct Parser<'a, 'b> { s: &'a [u8], i: usize, scope: &'a Scope<'b> }

impl<'a, 'b> Parser<'a, 'b> {
    fn skip_ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() { self.i += 1 }
    }

    fn expr(&mut self) -> Result<String, ExprError> {
        let mut acc = self.term()?;
        loop {
            self.skip_ws();
            if self.i < self.s.len() && self.s[self.i] == b'+' {
                self.i += 1;
                acc.push_str(&self.term()?);
            } else {
                return Ok(acc);
            }
        }
    }

    fn term(&mut self) -> Result<String, ExprError> {
        self.skip_ws();
        if self.i >= self.s.len() {
            return Err(ExprError::Syntax { message: "unexpected end of expression".into() });
        }
        match self.s[self.i] {
            b'"' => self.string(),
            b'{' => self.varref(),
            _ => self.funcall(),
        }
    }

    fn string(&mut self) -> Result<String, ExprError> {
        self.i += 1; // opening quote
        let start = self.i;
        while self.i < self.s.len() && self.s[self.i] != b'"' { self.i += 1 }
        if self.i >= self.s.len() {
            return Err(ExprError::Syntax { message: "unterminated string literal".into() });
        }
        let out = String::from_utf8_lossy(&self.s[start..self.i]).into_owned();
        self.i += 1; // closing quote
        Ok(out)
    }

    fn varref(&mut self) -> Result<String, ExprError> {
        let rest = std::str::from_utf8(&self.s[self.i..]).unwrap_or_default();
        if !rest.starts_with("{{") {
            return Err(ExprError::Syntax { message: "expected a variable reference".into() });
        }
        let end = rest.find("}}").ok_or(ExprError::Syntax { message: "unterminated variable reference".into() })?;
        let token = &rest[..end + 2];
        self.i += token.len();
        Ok(self.scope.interpolate(token)?)
    }

    fn funcall(&mut self) -> Result<String, ExprError> {
        let start = self.i;
        while self.i < self.s.len() && (self.s[self.i].is_ascii_alphanumeric() || self.s[self.i] == b'_') {
            self.i += 1
        }
        let name = String::from_utf8_lossy(&self.s[start..self.i]).into_owned();
        if name.is_empty() {
            return Err(ExprError::Syntax { message: format!("expected a function name at byte {start}") });
        }
        self.skip_ws();
        if self.i >= self.s.len() || self.s[self.i] != b'(' {
            return Err(ExprError::Syntax { message: format!("expected `(` after `{name}`") });
        }
        self.i += 1;
        let mut args = Vec::new();
        loop {
            self.skip_ws();
            if self.i < self.s.len() && self.s[self.i] == b')' { self.i += 1; break }
            args.push(self.expr()?);
            self.skip_ws();
            if self.i < self.s.len() && self.s[self.i] == b',' { self.i += 1; continue }
            if self.i < self.s.len() && self.s[self.i] == b')' { self.i += 1; break }
            return Err(ExprError::Syntax { message: format!("unterminated argument list for `{name}`") });
        }
        apply(&name, &args)
    }
}

fn one<'a>(name: &str, args: &'a [String]) -> Result<&'a str, ExprError> {
    match args {
        [a] => Ok(a.as_str()),
        _ => Err(ExprError::Arity { name: name.to_string(), expected: 1, found: args.len() }),
    }
}

fn apply(name: &str, args: &[String]) -> Result<String, ExprError> {
    match name {
        "md5" => {
            use md5::Digest;
            Ok(format!("{:x}", md5::Md5::digest(one(name, args)?.as_bytes())))
        }
        "sha1" => {
            use sha1::Digest;
            Ok(format!("{:x}", sha1::Sha1::digest(one(name, args)?.as_bytes())))
        }
        "sha256" => {
            use sha2::Digest;
            Ok(format!("{:x}", sha2::Sha256::digest(one(name, args)?.as_bytes())))
        }
        "base64" => Ok(base64::engine::general_purpose::STANDARD.encode(one(name, args)?.as_bytes())),
        "now" => Ok(chrono::Local::now().format(&to_chrono_format(one(name, args)?)).to_string()),
        other => Err(ExprError::UnknownFunction {
            name: other.to_string(),
            supported: SUPPORTED.join(", "),
        }),
    }
}

/// Maps the documented, user-facing format tokens to chrono's, longest token first.
fn to_chrono_format(fmt: &str) -> String {
    const MAP: &[(&str, &str)] = &[
        ("YYYY", "%Y"), ("MM", "%m"), ("DD", "%d"),
        ("HH", "%H"), ("mm", "%M"), ("ss", "%S"),
    ];
    let mut out = String::with_capacity(fmt.len());
    let mut rest = fmt;
    'outer: while !rest.is_empty() {
        for (from, to) in MAP {
            if rest.starts_with(from) {
                out.push_str(to);
                rest = &rest[from.len()..];
                continue 'outer;
            }
        }
        let ch = rest.chars().next().unwrap();
        if ch == '%' { out.push('%') } // escape a literal percent for chrono
        out.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    out
}
