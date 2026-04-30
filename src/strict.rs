//! Strictness analysis on De Bruijn expressions.
//!
//! `mark_strict` walks a closed `DBExpr` and converts plain `App` nodes to
//! `StrictApp` at every position where the bound parameter will provably
//! be forced when the function body is reduced to WHNF. The runtime then
//! evaluates strict args eagerly (skipping a thunk allocation on each
//! such β-step).
//!
//! # Analysis
//!
//! `head_strict_db(e, k)` returns the set of De Bruijn indices `< k` that
//! are guaranteed to be forced when `e` is evaluated to WHNF. Indices `< k`
//! are "the binders we just walked under" (the args of an uncurried call).
//!
//! Rules:
//!   - `Var(i)` (i < k)        → {i}
//!   - `Var(i)` (i >= k)        → {} (free wrt this call's binders)
//!   - `Abs(_, _)`              → {} (already a value; nothing forced)
//!   - `App(f, _)`              → head_strict(f, k) (head is forced first)
//!   - `StrictApp(f, _)`        → head_strict(f, k)
//!
//! # Spine annotation
//!
//! For each App-spine `((... (h a1) ...) an)` whose head `h` is a literal
//! `\x1...\xk. body` with `k <= n`, we treat the call as uncurried over
//! all `k` binders and mark the args at positions whose corresponding
//! De Bruijn index (counted from the *innermost* binder outward) appears
//! in `head_strict_db(body, k)`.

use crate::debruijn::DBExpr;

/// Public entry: walk `e` and return a structurally-equivalent term with
/// `App → StrictApp` rewrites at every position where strictness is
/// proven. The result is α-equivalent to `e` (since `StrictApp ≡ App`
/// under our `PartialEq`).
pub fn mark_strict(e: &DBExpr) -> DBExpr {
    match e {
        DBExpr::Var(_) => e.clone(),
        DBExpr::Abs(name, body) => DBExpr::abs(name.clone(), mark_strict(body)),
        DBExpr::App(_, _) | DBExpr::StrictApp(_, _) => mark_app(e),
        DBExpr::Fix(inner) => DBExpr::fix(mark_strict(inner)),
    }
}

/// Handle an App node: walk the spine, identify the head, and either
/// annotate strictness (if the head is a literal Abs of high enough
/// arity) or recurse blindly.
fn mark_app(e: &DBExpr) -> DBExpr {
    // Collect spine: head + args (innermost-arg-first means we collect
    // from outside in: the outermost App's arg is the outermost arg).
    let mut spine: Vec<&DBExpr> = Vec::new(); // args, OUTER first (= last to bind)
    let mut head: &DBExpr = e;
    while let DBExpr::App(f, x) | DBExpr::StrictApp(f, x) = head {
        spine.push(x);
        head = f;
    }
    // spine reversed: now innermost-arg first (= first to bind).
    spine.reverse();

    // Strip leading Abs binders from the head, up to spine.len() of them.
    let mut binders: Vec<String> = Vec::new();
    let mut body = head;
    while binders.len() < spine.len() {
        if let DBExpr::Abs(name, b) = body {
            binders.push(name.clone());
            body = b;
        } else {
            break;
        }
    }
    let k = binders.len(); // number of binders we've stripped

    // Compute strictness on the stripped body.
    let strict_indices = if k > 0 { head_strict_db(body, k) } else { Vec::new() };

    // Recursively mark_strict in the head and each arg.
    let head_marked = mark_strict(head);
    let args_marked: Vec<DBExpr> = spine.iter().map(|a| mark_strict(a)).collect();

    // Reassemble. spine order: innermost-first. Binder for arg i is at
    // De Bruijn index (k - 1 - i) from inside `body` (i.e., arg 0 is the
    // OUTERMOST binder, which has the largest index).
    let mut result = head_marked;
    for (i, arg) in args_marked.into_iter().enumerate() {
        let binder_index = if i < k { k - 1 - i } else { usize::MAX };
        let is_strict = i < k && strict_indices.contains(&binder_index);
        result = if is_strict {
            DBExpr::strict_app(result, arg)
        } else {
            DBExpr::app(result, arg)
        };
    }
    result
}

/// Return the De Bruijn indices `< k` that are guaranteed to be forced
/// when `e` is reduced to WHNF, treating the outer `k` binders as
/// just-introduced (uncurried-call view).
///
/// In practice the head spine contributes a single index (or none); we
/// return a `Vec` to keep the door open for combining heads in the future.
pub fn head_strict_db(e: &DBExpr, k: usize) -> Vec<usize> {
    fn go(e: &DBExpr, k: usize, depth: usize, out: &mut Vec<usize>) {
        match e {
            DBExpr::Var(i) => {
                // We're `depth` binders deeper than the strip point. An
                // index `i` here refers to binder `i - depth` in the
                // strip, when `i >= depth`.
                if *i >= depth {
                    let outer = *i - depth;
                    if outer < k && !out.contains(&outer) {
                        out.push(outer);
                    }
                }
            }
            DBExpr::Abs(_, _) => {
                // Reaching an Abs means WHNF — we don't peer inside.
            }
            DBExpr::App(f, _) | DBExpr::StrictApp(f, _) => {
                // Only the head of an application is forced (to determine
                // whether it's an Abs to apply). The arg may or may not
                // be forced — depends on what the head turns out to do.
                go(f, k, depth, out);
            }
            DBExpr::Fix(inner) => {
                // fix e ↪ e (fix e), which forces e first. Whatever e
                // forces, fix(e) also forces.
                go(inner, k, depth, out);
            }
        }
    }
    let mut out = Vec::new();
    go(e, k, 0, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dvar(i: usize) -> DBExpr {
        DBExpr::var(i)
    }
    fn dabs(name: &str, b: DBExpr) -> DBExpr {
        DBExpr::abs(name, b)
    }
    fn dapp(f: DBExpr, x: DBExpr) -> DBExpr {
        DBExpr::app(f, x)
    }

    // -------- head_strict_db --------

    #[test]
    fn head_strict_var_zero() {
        // body = 0 ; under k=1 stripped binders → strict on index 0
        assert_eq!(head_strict_db(&dvar(0), 1), vec![0]);
    }

    #[test]
    fn head_strict_app_head() {
        // body = 0 1 ; head is 0 → strict on index 0 only
        let body = dapp(dvar(0), dvar(1));
        assert_eq!(head_strict_db(&body, 2), vec![0]);
    }

    #[test]
    fn head_strict_under_abs_is_empty() {
        // body = \. 0 ; abs is WHNF, nothing forced
        let body = dabs("_", dvar(0));
        assert_eq!(head_strict_db(&body, 1), Vec::<usize>::new());
    }

    #[test]
    fn head_strict_var_above_k_is_ignored() {
        // body = 5 ; k=1 → no relevant index
        assert_eq!(head_strict_db(&dvar(5), 1), Vec::<usize>::new());
    }

    // -------- mark_strict --------

    #[test]
    fn mark_strict_identity_call() {
        // (\x. x) y → strict on the only arg (head-strict).
        // Names: \x. with body Var(0) ; applied to Var(99) (a free arg
        // standing in for some value).
        let e = dapp(dabs("x", dvar(0)), dvar(99));
        let marked = mark_strict(&e);
        // The result should be StrictApp.
        assert!(matches!(marked, DBExpr::StrictApp(_, _)));
    }

    #[test]
    fn mark_strict_const_call() {
        // (\x. \y. x) a b — uncurried view: body x, k=2, binders {x=index 1, y=index 0}.
        // head_strict(body=Var 1, k=2) = {1}. Arg 0 (innermost = `b` which
        // binds y at index 0) is NOT strict; arg 1 (outermost = `a` which
        // binds x at index 1) IS strict.
        // In our spine order: spine[0] = a (outermost binding, index k-1-0 = 1) → strict.
        //                      spine[1] = b (next,        index k-1-1 = 0) → not strict.
        let const_fn = dabs("x", dabs("y", dvar(1)));
        let e = dapp(dapp(const_fn, dvar(50)), dvar(60));
        let marked = mark_strict(&e);
        // Outermost App should be plain (b not strict); inner App should be StrictApp (a strict).
        match marked {
            DBExpr::App(inner, _) => match &*inner {
                DBExpr::StrictApp(_, _) => {}
                _ => panic!("expected inner StrictApp, got {:?}", inner),
            },
            _ => panic!("expected outer App, got {:?}", marked),
        }
    }

    #[test]
    fn mark_strict_lambda_arg_blocks_all() {
        // (\x. \y. y) a b — head_strict(body=Var 0, k=2) = {0}.
        // spine[0] = a binds x (index 1) → NOT strict.
        // spine[1] = b binds y (index 0) → strict.
        let const_snd = dabs("x", dabs("y", dvar(0)));
        let e = dapp(dapp(const_snd, dvar(50)), dvar(60));
        let marked = mark_strict(&e);
        match marked {
            DBExpr::StrictApp(inner, _) => match &*inner {
                DBExpr::App(_, _) => {}
                _ => panic!("expected inner App, got {:?}", inner),
            },
            _ => panic!("expected outer StrictApp, got {:?}", marked),
        }
    }

    #[test]
    fn mark_strict_under_abs_no_marking() {
        // Body that's already an Abs — applying to it under-arity means no
        // binders stripped, no marking.
        // (\x. \y. x) a — only one arg; body after stripping one binder is
        // \y. x which is Abs → head_strict empty → arg not strict.
        let const_fn = dabs("x", dabs("y", dvar(1)));
        let e = dapp(const_fn, dvar(50));
        let marked = mark_strict(&e);
        assert!(matches!(marked, DBExpr::App(_, _)));
    }

    #[test]
    fn mark_strict_recurses_into_subterms() {
        // \z. (\x. x) y — should mark the inner App as Strict.
        let inner = dapp(dabs("x", dvar(0)), dvar(1));
        let outer = dabs("z", inner);
        let marked = mark_strict(&outer);
        if let DBExpr::Abs(_, body) = marked {
            assert!(matches!(*body, DBExpr::StrictApp(_, _)));
        } else {
            panic!("expected outer Abs");
        }
    }
}
