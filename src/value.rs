//! Runtime values.

use std::cell::RefCell;
use std::collections::HashSet;
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
    Char(char),
    /// `Rc`, not a plain `String`: `eq?` (spec 3.7) must tell two
    /// separately-built strings with identical contents (different
    /// objects) apart from the same string bound to two different names
    /// (the same object) — a plain owned `String`, deep-cloned on every
    /// `Value::clone()`, has no notion of object identity to compare.
    Str(Rc<String>),
    Symbol(String),
    Native(String),
    /// A user-defined function: an index into the module's function table,
    /// plus the environment it closed over at creation time (empty/parentless
    /// for a top-level definition, which has no enclosing locals to capture).
    Closure(u32, Rc<Env>),
    /// A cons cell (spec 5.1): its two halves can be replaced in place after
    /// construction (`set-car!`/`set-cdr!`), so it needs interior
    /// mutability, not just `Rc`'s shared-ownership identity (the same
    /// `eq?`-identity reason as `Str` above).
    Pair(Rc<RefCell<(Value, Value)>>),
    /// `Rc`, not a plain `Vec`, for the same `eq?`-identity reason as `Str`
    /// and `Pair` above — a non-empty list is a compound/reference value
    /// per spec 3.7, so two separately-quoted lists with the same contents
    /// must NOT be `eq?` to each other.
    List(Rc<Vec<Value>>),
    /// A fixed-length mutable array (spec 4.5). Minimal: enough to exist as
    /// a distinct, `eq?`-by-reference, `vector?`-recognizable value type;
    /// the full vector-manipulation procedure library is a later behaviour.
    Vector(Rc<RefCell<Vec<Value>>>),
    /// A mutable hash table (spec 4.6), keyed by `equal?`. Minimal:
    /// association-list-backed, enough to exist as a distinct,
    /// `eq?`-by-reference, `hash?`-recognizable value type; the full
    /// hash-table procedure library (`hash-set!`, `hash-ref`, ...) is a
    /// later behaviour.
    Hash(Rc<RefCell<Vec<(Value, Value)>>>),
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

/// Prints a pair the way a proper (possibly improper-tailed) list reads:
/// walking the cdr chain space-separating each car, switching to a trailing
/// `. tail` only once the chain ends in something other than the empty
/// list -- rather than the raw `(a . b)` a single cons cell would suggest.
/// No cycle detection: terminates on acyclic data, as required; behaviour
/// on cyclic data is implementation-defined per spec.
fn display_pair_chain(
    f: &mut fmt::Formatter<'_>,
    cell: &Rc<RefCell<(Value, Value)>>,
) -> fmt::Result {
    write!(f, "(")?;
    let first = cell.borrow().0.clone();
    write!(f, "{first}")?;
    let mut current = cell.borrow().1.clone();
    loop {
        match current {
            Value::Pair(next) => {
                let (car, cdr) = {
                    let borrowed = next.borrow();
                    (borrowed.0.clone(), borrowed.1.clone())
                };
                write!(f, " {car}")?;
                current = cdr;
            }
            Value::List(items) if items.is_empty() => break,
            Value::List(items) => {
                for item in items.iter() {
                    write!(f, " {item}")?;
                }
                break;
            }
            other => {
                write!(f, " . {other}")?;
                break;
            }
        }
    }
    write!(f, ")")
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{n}"),
            Value::Float(n) => write!(f, "{}", format_float(*n)),
            Value::Bool(true) => write!(f, "#t"),
            Value::Bool(false) => write!(f, "#f"),
            Value::Char(c) => write!(f, "{c}"),
            Value::Str(s) => write!(f, "{s}"),
            Value::Symbol(s) => write!(f, "{s}"),
            Value::Native(name) => write!(f, "#<procedure:{name}>"),
            Value::Closure(idx, _) => write!(f, "#<procedure:{idx}>"),
            Value::Pair(cell) => display_pair_chain(f, cell),
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
            Value::Vector(items) => {
                write!(f, "#(")?;
                for (i, item) in items.borrow().iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, ")")
            }
            Value::Hash(_) => write!(f, "#<hash>"),
            Value::Unspecified => Ok(()),
        }
    }
}

pub fn is_truthy(value: &Value) -> bool {
    !matches!(value, Value::Bool(false))
}

/// NaN compares equal to NaN under `eqv?`; positive and negative zero do
/// NOT compare equal to each other (spec 3.7) -- a bit-pattern comparison,
/// not IEEE `==`, which disagrees with both of those on purpose.
fn float_eqv(a: f64, b: f64) -> bool {
    (a.is_nan() && b.is_nan()) || a.to_bits() == b.to_bits()
}

/// The shared implementation behind both `eq?` and `eqv?` (spec 3.7):
/// simple values (fixnums, booleans, characters, symbols, the empty list)
/// compare by value; compound values (pairs, strings, vectors, hashes,
/// non-empty lists, procedures) compare only if they're literally the same
/// object. No demo in this language's behaviour suite distinguishes `eq?`
/// from `eqv?` on any concrete input -- `eq?` on floats is explicitly
/// implementation-defined, and this implementation picks the same
/// bit-precise comparison `eqv?` requires, so one function correctly backs
/// both native procedures.
pub fn value_eqv(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Char(x), Value::Char(y)) => x == y,
        (Value::Symbol(x), Value::Symbol(y)) => x == y,
        (Value::Native(x), Value::Native(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => float_eqv(*x, *y),
        (Value::List(x), Value::List(y)) if x.is_empty() && y.is_empty() => true,
        (Value::Str(x), Value::Str(y)) => Rc::ptr_eq(x, y),
        (Value::Pair(x), Value::Pair(y)) => Rc::ptr_eq(x, y),
        (Value::List(x), Value::List(y)) => Rc::ptr_eq(x, y),
        (Value::Vector(x), Value::Vector(y)) => Rc::ptr_eq(x, y),
        (Value::Hash(x), Value::Hash(y)) => Rc::ptr_eq(x, y),
        (Value::Closure(idx1, env1), Value::Closure(idx2, env2)) => {
            idx1 == idx2 && Rc::ptr_eq(env1, env2)
        }
        (Value::Unspecified, Value::Unspecified) => true,
        _ => false,
    }
}

/// Decomposes any non-empty proper-or-improper-list-shaped value into its
/// car/cdr, regardless of whether it's backed by a `Pair` chain or a flat
/// `List` -- so `value_equal` can walk a list uniformly without caring
/// which representation either side happens to use (spec 5.1's own
/// BOUNDARIES: that choice isn't observable).
fn as_pair_parts(v: &Value) -> Option<(Value, Value)> {
    match v {
        Value::Pair(cell) => {
            let borrowed = cell.borrow();
            Some((borrowed.0.clone(), borrowed.1.clone()))
        }
        Value::List(items) if !items.is_empty() => {
            Some((items[0].clone(), Value::List(Rc::new(items[1..].to_vec()))))
        }
        _ => None,
    }
}

/// Deep structural equality (spec 3.7): walks into pairs, vectors, and
/// strings comparing contents, falling back to `eqv?` for everything else.
///
/// Explicitly iterative (a heap-allocated work stack), not recursive: a
/// `Pair` chain has no runtime length bound -- an ordinary, non-malicious
/// program can build an arbitrarily long one via `cons` in a tail-recursive
/// loop -- so one native stack frame per element would let ordinary source
/// text crash the process outright (warden security review, msg #144),
/// bypassing this project's own panic-catching defense in depth (a Rust
/// stack overflow aborts the process unconditionally; it is never a caught
/// panic). A `seen` set of already-compared `Pair` address pairs also makes
/// this safe on a *cyclic* pair chain (constructible since pairs became
/// mutable via `set-car!`/`set-cdr!`): revisiting the same pair of
/// addresses means the walk has gone all the way around the cycle without
/// finding a mismatch, so it's correct to treat that branch as equal and
/// stop there instead of looping forever.
pub fn value_equal(a: &Value, b: &Value) -> bool {
    let mut seen: HashSet<(usize, usize)> = HashSet::new();
    let mut work: Vec<(Value, Value)> = vec![(a.clone(), b.clone())];
    while let Some((x, y)) = work.pop() {
        if let (Value::Pair(px), Value::Pair(py)) = (&x, &y) {
            let key = (Rc::as_ptr(px) as usize, Rc::as_ptr(py) as usize);
            if !seen.insert(key) {
                continue;
            }
        }
        if let (Some((x0, x1)), Some((y0, y1))) = (as_pair_parts(&x), as_pair_parts(&y)) {
            work.push((x1, y1));
            work.push((x0, y0));
            continue;
        }
        match (&x, &y) {
            (Value::Str(sx), Value::Str(sy)) => {
                if sx != sy {
                    return false;
                }
            }
            (Value::List(lx), Value::List(ly)) if lx.is_empty() && ly.is_empty() => {}
            (Value::Vector(vx), Value::Vector(vy)) => {
                let vx = vx.borrow();
                let vy = vy.borrow();
                if vx.len() != vy.len() {
                    return false;
                }
                for (ex, ey) in vx.iter().zip(vy.iter()) {
                    work.push((ex.clone(), ey.clone()));
                }
            }
            (x2, y2) => {
                if !value_eqv(x2, y2) {
                    return false;
                }
            }
        }
    }
    true
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
            Value::Str(Rc::new("hello\nworld".to_string())).to_string(),
            "hello\nworld"
        );
    }

    #[test]
    fn displays_a_native_procedure_with_its_name() {
        assert_eq!(Value::Native("+".to_string()).to_string(), "#<procedure:+>");
    }

    #[test]
    fn displays_an_empty_list_as_a_pair_of_parens() {
        assert_eq!(Value::List(Rc::new(vec![])).to_string(), "()");
    }

    #[test]
    fn displays_a_list_with_space_separated_elements() {
        let list = Value::List(Rc::new(vec![
            Value::Symbol("+".to_string()),
            Value::Int(1),
            Value::Int(2),
        ]));
        assert_eq!(list.to_string(), "(+ 1 2)");
    }

    #[test]
    fn displays_an_empty_vector_with_no_interior_space() {
        assert_eq!(
            Value::Vector(Rc::new(RefCell::new(vec![]))).to_string(),
            "#()"
        );
    }

    #[test]
    fn displays_a_vector_with_space_separated_elements() {
        let vector = Value::Vector(Rc::new(RefCell::new(vec![
            Value::Int(1),
            Value::Int(2),
            Value::Int(3),
        ])));
        assert_eq!(vector.to_string(), "#(1 2 3)");
    }

    #[test]
    fn displays_a_dotted_pair_with_the_dot_notation() {
        let pair = Value::Pair(Rc::new(RefCell::new((Value::Int(1), Value::Int(2)))));
        assert_eq!(pair.to_string(), "(1 . 2)");
    }

    #[test]
    fn displays_a_proper_list_built_from_pairs_without_dot_notation() {
        let list = Value::Pair(Rc::new(RefCell::new((
            Value::Int(1),
            Value::Pair(Rc::new(RefCell::new((
                Value::Int(2),
                Value::List(Rc::new(vec![])),
            )))),
        ))));
        assert_eq!(list.to_string(), "(1 2)");
    }

    #[test]
    fn displays_an_improper_list_with_a_trailing_dotted_tail() {
        let list = Value::Pair(Rc::new(RefCell::new((
            Value::Int(1),
            Value::Pair(Rc::new(RefCell::new((Value::Int(2), Value::Int(3))))),
        ))));
        assert_eq!(list.to_string(), "(1 2 . 3)");
    }

    #[test]
    fn displays_a_pair_whose_cdr_is_a_non_empty_list_inline_not_dotted() {
        let pair = Value::Pair(Rc::new(RefCell::new((
            Value::Int(1),
            Value::List(Rc::new(vec![Value::Int(2), Value::Int(3)])),
        ))));
        assert_eq!(pair.to_string(), "(1 2 3)");
    }

    #[test]
    fn displays_a_nested_list_recursively() {
        let list = Value::List(Rc::new(vec![
            Value::Int(1),
            Value::List(Rc::new(vec![Value::Int(2), Value::Int(3)])),
        ]));
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
        assert!(is_truthy(&Value::List(Rc::new(vec![]))));
        assert!(is_truthy(&Value::Str(Rc::new(String::new()))));
        assert!(is_truthy(&Value::Unspecified));
    }
}
