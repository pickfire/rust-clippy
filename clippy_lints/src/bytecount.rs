use rustc::hir::*;
use rustc::lint::*;
use rustc::ty;
use syntax::ast::{Name, UintTy};
use crate::utils::{contains_name, get_pat_name, match_type, paths, single_segment_path, snippet, span_lint_and_sugg,
            walk_ptrs_ty};

/// **What it does:** Checks for naive byte counts
///
/// **Why is this bad?** The [`bytecount`](https://crates.io/crates/bytecount)
/// crate has methods to count your bytes faster, especially for large slices.
///
/// **Known problems:** If you have predominantly small slices, the
/// `bytecount::count(..)` method may actually be slower. However, if you can
/// ensure that less than 2³²-1 matches arise, the `naive_count_32(..)` can be
/// faster in those cases.
///
/// **Example:**
///
/// ```rust
/// &my_data.filter(|&x| x == 0u8).count() // use bytecount::count instead
/// ```
declare_clippy_lint! {
    pub NAIVE_BYTECOUNT,
    perf,
    "use of naive `<slice>.filter(|&x| x == y).count()` to count byte values"
}

#[derive(Copy, Clone)]
pub struct ByteCount;

impl LintPass for ByteCount {
    fn get_lints(&self) -> LintArray {
        lint_array!(NAIVE_BYTECOUNT)
    }
}

impl<'a, 'tcx> LateLintPass<'a, 'tcx> for ByteCount {
    fn check_expr(&mut self, cx: &LateContext, expr: &Expr) {
        if_chain! {
            if let ExprKind::MethodCall(ref count, _, ref count_args) = expr.node;
            if count.ident.name == "count";
            if count_args.len() == 1;
            if let ExprKind::MethodCall(ref filter, _, ref filter_args) = count_args[0].node;
            if filter.ident.name == "filter";
            if filter_args.len() == 2;
            if let ExprKind::Closure(_, _, body_id, _, _) = filter_args[1].node;
            then {
                let body = cx.tcx.hir.body(body_id);
                if_chain! {
                    if body.arguments.len() == 1;
                    if let Some(argname) = get_pat_name(&body.arguments[0].pat);
                    if let ExprKind::Binary(ref op, ref l, ref r) = body.value.node;
                    if op.node == BinOpKind::Eq;
                    if match_type(cx,
                               walk_ptrs_ty(cx.tables.expr_ty(&filter_args[0])),
                               &paths::SLICE_ITER);
                    then {
                        let needle = match get_path_name(l) {
                            Some(name) if check_arg(name, argname, r) => r,
                            _ => match get_path_name(r) {
                                Some(name) if check_arg(name, argname, l) => l,
                                _ => { return; }
                            }
                        };
                        if ty::TyUint(UintTy::U8) != walk_ptrs_ty(cx.tables.expr_ty(needle)).sty {
                            return;
                        }
                        let haystack = if let ExprKind::MethodCall(ref path, _, ref args) =
                                filter_args[0].node {
                            let p = path.ident.name;
                            if (p == "iter" || p == "iter_mut") && args.len() == 1 {
                                &args[0]
                            } else {
                                &filter_args[0]
                            }
                        } else {
                            &filter_args[0]
                        };
                        span_lint_and_sugg(cx,
                                           NAIVE_BYTECOUNT,
                                           expr.span,
                                           "You appear to be counting bytes the naive way",
                                           "Consider using the bytecount crate",
                                           format!("bytecount::count({}, {})",
                                                    snippet(cx, haystack.span, ".."),
                                                    snippet(cx, needle.span, "..")));
                    }
                };
            }
        };
    }
}

fn check_arg(name: Name, arg: Name, needle: &Expr) -> bool {
    name == arg && !contains_name(name, needle)
}

fn get_path_name(expr: &Expr) -> Option<Name> {
    match expr.node {
        ExprKind::Box(ref e) | ExprKind::AddrOf(_, ref e) | ExprKind::Unary(UnOp::UnDeref, ref e) => get_path_name(e),
        ExprKind::Block(ref b, _) => if b.stmts.is_empty() {
            b.expr.as_ref().and_then(|p| get_path_name(p))
        } else {
            None
        },
        ExprKind::Path(ref qpath) => single_segment_path(qpath).map(|ps| ps.ident.name),
        _ => None,
    }
}
