//! Runtime values.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Bool(bool),
    Str(String),
    Symbol(String),
    Native(String),
    Unspecified,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{n}"),
            Value::Bool(true) => write!(f, "true"),
            Value::Bool(false) => write!(f, "false"),
            Value::Str(s) => write!(f, "{s}"),
            Value::Symbol(s) => write!(f, "{s}"),
            Value::Native(name) => write!(f, "#<procedure:{name}>"),
            Value::Unspecified => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displays_an_integer() {
        assert_eq!(Value::Int(42).to_string(), "42");
    }

    #[test]
    fn displays_a_negative_integer() {
        assert_eq!(Value::Int(-7).to_string(), "-7");
    }

    #[test]
    fn displays_booleans_as_true_and_false() {
        assert_eq!(Value::Bool(true).to_string(), "true");
        assert_eq!(Value::Bool(false).to_string(), "false");
    }

    #[test]
    fn displays_a_string_as_its_raw_content() {
        assert_eq!(
            Value::Str("hello\nworld".to_string()).to_string(),
            "hello\nworld"
        );
    }

    #[test]
    fn displays_a_native_procedure_with_its_name() {
        assert_eq!(Value::Native("+".to_string()).to_string(), "#<procedure:+>");
    }
}
