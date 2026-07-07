//! Runtime values.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

/// The chain of captured local-variable cells a closure closes over: this
/// frame's own locals (shared, not copied, with whatever call created them —
/// still live and mutable even after that call has returned), plus a link
/// to whatever the CREATING frame had itself captured, so a closure nested
/// more than one level deep can still reach an outer ancestor's variables.
#[derive(Debug, Clone, PartialEq)]
pub struct Env {
    pub locals: Vec<Rc<RefCell<Value>>>,
    pub parent: Option<Rc<Env>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Symbol(String),
    Native(String),
    /// A user-defined function: an index into the module's function table,
    /// plus the environment it closed over at creation time (empty/parentless
    /// for a top-level definition, which has no enclosing locals to capture).
    Closure(u32, Rc<Env>),
    /// A minimal cons cell — just enough to construct a pair and retrieve
    /// each half back out; the broader pair/list operation library is a
    /// later behaviour, so this deliberately isn't unified with `List`.
    Pair(Box<Value>, Box<Value>),
    List(Vec<Value>),
    Unspecified,
}

/// Formats a float per spec: the shortest decimal text that reads back to
/// the exact same value (delegated to Rust's own Display for f64, which
/// already implements shortest-round-trip digit generation — reimplementing
/// that algorithm by hand would just be a worse copy of it), always with an
/// explicit decimal point (a whole-valued float still shows a trailing
/// `.0`), switching to exponential notation outside the ordinary magnitude
/// range [1e-3, 1e15], with dedicated forms for the special values and for
/// negative zero (which Rust's own Display collapses to "-0", losing the
/// distinction this language needs to preserve).
fn format_float(n: f64) -> String {
    if n.is_nan() {
        return "+nan.0".to_string();
    }
    if n.is_infinite() {
        return if n.is_sign_positive() {
            "+inf.0".to_string()
        } else {
            "-inf.0".to_string()
        };
    }
    if n == 0.0 {
        return if n.is_sign_negative() {
            "-0.0".to_string()
        } else {
            "0.0".to_string()
        };
    }
    if (1e-3..=1e15).contains(&n.abs()) {
        let plain = format!("{n}");
        if plain.contains('.') {
            plain
        } else {
            format!("{plain}.0")
        }
    } else {
        format!("{n:e}")
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{n}"),
            Value::Float(n) => write!(f, "{}", format_float(*n)),
            Value::Bool(true) => write!(f, "#t"),
            Value::Bool(false) => write!(f, "#f"),
            Value::Str(s) => write!(f, "{s}"),
            Value::Symbol(s) => write!(f, "{s}"),
            Value::Native(name) => write!(f, "#<procedure:{name}>"),
            Value::Closure(idx, _) => write!(f, "#<procedure:{idx}>"),
            Value::Pair(a, b) => write!(f, "({a} . {b})"),
            Value::List(items) => {
                write!(f, "(")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, ")")
            }
            Value::Unspecified => Ok(()),
        }
    }
}

pub fn is_truthy(value: &Value) -> bool {
    !matches!(value, Value::Bool(false))
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
    fn displays_booleans_as_hash_t_and_hash_f() {
        assert_eq!(Value::Bool(true).to_string(), "#t");
        assert_eq!(Value::Bool(false).to_string(), "#f");
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

    #[test]
    fn displays_an_empty_list_as_a_pair_of_parens() {
        assert_eq!(Value::List(vec![]).to_string(), "()");
    }

    #[test]
    fn displays_a_list_with_space_separated_elements() {
        let list = Value::List(vec![
            Value::Symbol("+".to_string()),
            Value::Int(1),
            Value::Int(2),
        ]);
        assert_eq!(list.to_string(), "(+ 1 2)");
    }

    #[test]
    fn displays_a_nested_list_recursively() {
        let list = Value::List(vec![
            Value::Int(1),
            Value::List(vec![Value::Int(2), Value::Int(3)]),
        ]);
        assert_eq!(list.to_string(), "(1 (2 3))");
    }

    #[test]
    fn displays_a_whole_valued_float_with_a_trailing_dot_zero() {
        assert_eq!(Value::Float(1.0).to_string(), "1.0");
        assert_eq!(Value::Float(-3.0).to_string(), "-3.0");
    }

    #[test]
    fn displays_a_fractional_float_with_its_shortest_round_tripping_digits() {
        assert_eq!(Value::Float(3.5).to_string(), "3.5");
        assert_eq!(Value::Float(0.1).to_string(), "0.1");
    }

    #[test]
    fn displays_positive_and_negative_zero_distinctly() {
        assert_eq!(Value::Float(0.0).to_string(), "0.0");
        assert_eq!(Value::Float(-0.0).to_string(), "-0.0");
    }

    #[test]
    fn displays_the_special_float_values_in_their_dedicated_forms() {
        assert_eq!(Value::Float(f64::NAN).to_string(), "+nan.0");
        assert_eq!(Value::Float(f64::INFINITY).to_string(), "+inf.0");
        assert_eq!(Value::Float(f64::NEG_INFINITY).to_string(), "-inf.0");
    }

    #[test]
    fn displays_ordinary_magnitudes_in_plain_non_exponent_notation() {
        // The boundary itself (1e15) is still plain, per spec's inclusive
        // [1e-3, 1e15] range.
        assert_eq!(Value::Float(1e15).to_string(), "1000000000000000.0");
        assert_eq!(Value::Float(1e-3).to_string(), "0.001");
    }

    #[test]
    fn switches_to_exponential_notation_outside_the_ordinary_magnitude_range() {
        assert_eq!(Value::Float(1e16).to_string(), "1e16");
        assert_eq!(Value::Float(1e-4).to_string(), "1e-4");
    }

    #[test]
    fn only_the_boolean_false_is_falsy() {
        assert!(!is_truthy(&Value::Bool(false)));
        assert!(is_truthy(&Value::Bool(true)));
        assert!(is_truthy(&Value::Int(0)));
        assert!(is_truthy(&Value::List(vec![])));
        assert!(is_truthy(&Value::Str(String::new())));
        assert!(is_truthy(&Value::Unspecified));
    }
}
