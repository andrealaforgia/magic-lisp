//! Step definitions for features/B9-pairs-and-lists.feature.

use super::registry::Registry;
use super::world::eval_ok;

pub(crate) fn registry() -> Registry {
    Registry::new()
        // --- E1 ---
        .step(
            "a constructed pair, and nested pairs reachable via 2- and 3-level accessor composition",
            |_w, _text, _| {},
        )
        .step(
            "each half of the pair is mutated in place, and the accessors are applied to the nested pairs",
            |w, _text, _| {
                let out = eval_ok(
                    "b9-e1.ml",
                    "(define p (cons 1 2)) \
                     (display (car p)) (display (cdr p)) \
                     (set-car! p 99) (set-cdr! p 100) \
                     (display (car p)) (display (cdr p)) \
                     (display (cadr (cons 1 (cons 2 3)))) \
                     (display (cddr (cons 1 (cons 2 3)))) \
                     (display (caar (cons (cons 10 20) 3))) \
                     (display (caddr (cons 1 (cons 2 (cons 3 4)))))",
                );
                w.notes.push(out);
            },
        )
        .step(
            "the mutations are observed afterward, and each accessor correctly reaches the value at its composed depth",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "129910023103");
            },
        )
        // --- E2 ---
        .step("lists built from a sequence of values", |_w, _text, _| {})
        .step(
            "length, append, reverse, list-ref, list-tail, and last-pair are applied",
            |w, _text, _| {
                let out = eval_ok(
                    "b9-e2.ml",
                    "(display (length (quote (a b c)))) \
                     (display (append (list 1 2) (list 3 4))) \
                     (display (reverse (list 1 2 3))) \
                     (display (list-ref (list 10 20 30) 1)) \
                     (display (list-ref (list 10 20 30) 2)) \
                     (display (list-tail (list 1 2 3) 0)) \
                     (display (list-tail (list 1 2 3) 2)) \
                     (display (last-pair (list 1 2 3)))",
                );
                w.notes.push(out);
            },
        )
        .step(
            "each returns the correct value, list-tail at 0 returns the list unchanged, and last-pair returns the final PAIR (still cons-shaped), not just the bare last element",
            |w, _text, _| {
                assert_eq!(
                    w.notes.last().unwrap(),
                    "3(1 2 3 4)(3 2 1)2030(1 2 3)(3)(3)"
                );
            },
        )
        // --- E3 ---
        .step(
            "a list containing a compound element, searched by identity (memq), by eqv?-level (memv), and by structural equality (member)",
            |_w, _text, _| {},
        )
        .step("each search is applied to a matching element", |w, _text, _| {
            let out = eval_ok(
                "b9-e3.ml",
                "(display (member 2 (list 1 2 3))) \
                 (display (member (list 1 2) (list (list 1 2) 3))) \
                 (display (memq (list 1 2) (list (list 1 2) 3))) \
                 (display (memv 2 (list 1 2 3)))",
            );
            w.notes.push(out);
        })
        .step(
            "member finds a structurally-equal-but-different-object element that memq cannot, memv is demonstrated present and correct, and all three agree on simple values",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "(2 3)((1 2) 3)#f(2 3)");
            },
        )
        // --- E4 ---
        .step(
            "an association list containing a compound key, searched by identity (assq), by eqv?-level (assv), and by structural equality (assoc)",
            |_w, _text, _| {},
        )
        .step("each search is applied to a matching key", |w, _text, _| {
            let out = eval_ok(
                "b9-e4.ml",
                "(display (assoc 2 (list (cons 1 (quote a)) (cons 2 (quote b))))) \
                 (display (assoc (list 1 2) (list (cons (list 1 2) (quote a))))) \
                 (display (assq (list 1 2) (list (cons (list 1 2) (quote a))))) \
                 (display (assv 2 (list (cons 1 (quote a)) (cons 2 (quote b)))))",
            );
            w.notes.push(out);
        })
        .step(
            "assoc finds a structurally-equal-but-different-object key that assq cannot, assv is demonstrated present and correct, and all three agree on simple keys",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "(2 . b)((1 2) . a)#f(2 . b)");
            },
        )
        // --- E5 ---
        .step(
            "a function applied to one list, to two lists in parallel, and used as a side-effecting iteration, plus a predicate used to keep matching elements",
            |_w, _text, _| {},
        )
        .step("map, for-each, and filter are each applied", |w, _text, _| {
            let out = eval_ok(
                "b9-e5.ml",
                "(display (map (lambda (x) (* x x)) (list 1 2 3))) \
                 (display (map + (list 1 2 3) (list 10 20 30))) \
                 (display (filter odd? (list 1 2 3 4 5))) \
                 (for-each (lambda (x) (display x)) (list 1 2 3)) (newline) \
                 (display (for-each (lambda (x) x) (list 1 2 3))) \
                 (display (map (lambda (x) x) (list 1 2 3)))",
            );
            w.notes.push(out);
        })
        .step(
            "map produces a new list (including the two-list parallel case), filter keeps only matching elements, and for-each's own expression value is NOT a list (displays as nothing) even though its side effects still occur in order — unlike map on the same transformation",
            |w, _text, _| {
                assert_eq!(
                    w.notes.last().unwrap(),
                    "(1 4 9)(11 22 33)(1 3 5)123\n(1 2 3)"
                );
            },
        )
        // --- E6 ---
        .step(
            "a non-commutative operation folded over the same list from the left and from the right, and reduce given a non-identity initial value on both an empty and a non-empty list",
            |_w, _text, _| {},
        )
        .step("each reduction is applied", |w, _text, _| {
            let out = eval_ok(
                "b9-e6.ml",
                "(display (fold-left + 0 (list 1 2 3 4))) \
                 (display (fold-right cons (quote ()) (list 1 2 3))) \
                 (display (fold-left - 0 (list 1 2 3))) \
                 (display (fold-right - 0 (list 1 2 3))) \
                 (display (reduce + 0 (list 1 2 3 4))) \
                 (display (reduce + 99 (quote ()))) \
                 (display (reduce + 99 (list 1 2 3))) \
                 (display (fold-left + 99 (list 1 2 3)))",
            );
            w.notes.push(out);
        })
        .step(
            "fold-left and fold-right produce different results on the same non-commutative input (proving real left/right evaluation order), and reduce ignores its initial value on a non-empty list (seeding from the list's own first element) while using it as the result for an empty list",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "10(1 2 3)-6210996105");
            },
        )
        // --- E7 ---
        .step(
            "a function called with two direct arguments plus a trailing list, with just a trailing list, and with an empty trailing list",
            |_w, _text, _| {},
        )
        .step("apply is used in each case", |w, _text, _| {
            let out = eval_ok(
                "b9-e7.ml",
                "(display (apply + 1 2 (list 3 4))) \
                 (display (apply + (list 1 2 3))) \
                 (display (apply + 1 2 (list)))",
            );
            w.notes.push(out);
        })
        .step(
            "all arguments are passed as one flat set regardless of how many came directly versus from the list",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "1063");
            },
        )
        // --- E8 ---
        .step(
            "a quoted literal containing a nested list, and quoted dotted (improper) pair literals",
            |_w, _text, _| {},
        )
        .step("the literals are read and inspected", |w, _text, _| {
            let out = eval_ok(
                "b9-e8.ml",
                "(display (equal? (quote (1 (2 3) 4)) \
                                  (cons 1 (cons (cons 2 (cons 3 (quote ()))) (cons 4 (quote ())))))) \
                 (display (car (cadr (quote (1 (2 3) 4))))) \
                 (display (quote (a . b))) \
                 (display (quote (1 2 . 3))) \
                 (display (list? (quote (1 2 . 3))))",
            );
            w.notes.push(out);
        })
        .step(
            "the nested literal is structurally identical to the equivalent hand-built cons structure and its nested part is reachable, and the dotted literals display and are recognized as improper (not proper lists)",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "#t2(a . b)(1 2 . 3)#f");
            },
        )
        // --- E9 ---
        .step(
            "all fourteen DEMO expressions from the behaviour spec run together in one program",
            |_w, _text, _| {},
        )
        .step("it is run", |w, _text, _| {
            let out = eval_ok(
                "b9-e9.ml",
                "(display (car (quote (1 2 3)))) (newline) \
                 (display (cadr (quote (1 2 3)))) (newline) \
                 (display (length (quote (a b c)))) (newline) \
                 (display (append (list 1 2) (list 3 4))) (newline) \
                 (display (reverse (list 1 2 3))) (newline) \
                 (display (map (lambda (x) (* x x)) (list 1 2 3))) (newline) \
                 (display (map + (list 1 2 3) (list 10 20 30))) (newline) \
                 (display (filter odd? (list 1 2 3 4 5))) (newline) \
                 (display (fold-left + 0 (list 1 2 3 4))) (newline) \
                 (display (fold-right cons (quote ()) (list 1 2 3))) (newline) \
                 (display (reduce + 0 (list 1 2 3 4))) (newline) \
                 (display (apply + 1 2 (list 3 4))) (newline) \
                 (display (assoc 2 (list (cons 1 (quote a)) (cons 2 (quote b))))) (newline) \
                 (display (member 2 (list 1 2 3))) (newline)",
            );
            w.notes.push(out);
        })
        .step(
            "each line of output matches its prescribed value exactly, and the process exits 0",
            |w, _text, _| {
                assert_eq!(
                    w.notes.last().unwrap(),
                    "1\n2\n3\n(1 2 3 4)\n(3 2 1)\n(1 4 9)\n(11 22 33)\n(1 3 5)\n\
                     10\n(1 2 3)\n10\n10\n(2 . b)\n(2 3)\n"
                );
            },
        )
}
