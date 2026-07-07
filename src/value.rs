//! Runtime values.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Bool(bool),
    Str(String),
    Symbol(String),
    Native(String),
    /// A user-defined function: an index into the module's function table.
    Function(u32),
    List(Vec<Value>),
    Unspecified,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{n}"),
            Value::Bool(true) => write!(f, "#t"),
            Value::Bool(false) => write!(f, "#f"),
            Value::Str(s) => write!(f, "{s}"),
            Value::Symbol(s) => write!(f, "{s}"),
            Value::Native(name) => write!(f, "#<procedure:{name}>"),
            Value::Function(idx) => write!(f, "#<procedure:{idx}>"),
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
    fn only_the_boolean_false_is_falsy() {
        assert!(!is_truthy(&Value::Bool(false)));
        assert!(is_truthy(&Value::Bool(true)));
        assert!(is_truthy(&Value::Int(0)));
        assert!(is_truthy(&Value::List(vec![])));
        assert!(is_truthy(&Value::Str(String::new())));
        assert!(is_truthy(&Value::Unspecified));
    }
}
