use std::rc::Rc;
use std::sync::Arc;

use crate::doc::*;

#[test]
fn doc_group_fits_flat() {
    let doc = group(seq([txt("a"), boxed(line()), txt("b")]));
    assert_eq!(render(doc, 10), "a b");
}

#[test]
fn doc_group_breaks_when_needed() {
    let doc = group(seq([txt("a"), boxed(line()), txt("b")]));
    assert_eq!(render(doc, 1), "a\nb");
}

#[test]
fn doc_nest_indents_after_hardline() {
    let doc = seq([txt("a"), boxed(nest(4, seq([hard(), txt("b")])))]);
    assert_eq!(render(doc, 80), "a\n    b");
}

#[test]
fn doc_flat_alt_uses_flat_branch_in_group() {
    let doc = group(flat_alt(txt(" "), hard()));
    assert_eq!(render(doc, 80), " ");
}

#[test]
fn doc_fail_forces_group_to_break() {
    let doc = group(flat_alt(fail(), txt("broken")));
    assert_eq!(render(doc, 80), "broken");
}

#[test]
fn doc_supports_box_rc_and_arc() {
    let left: Box<dyn Doc> = txt("a");
    let right: Rc<dyn Doc> = Rc::new(Text { s: "b".to_string() });
    let end: Arc<dyn Doc> = Arc::new(Text { s: "c".to_string() });
    assert_eq!(render(seq([left, boxed(right), boxed(end)]), 80), "abc");
}
