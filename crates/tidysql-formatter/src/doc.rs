use std::rc::Rc;
use std::sync::Arc;

pub(crate) trait Doc {
    fn render(&self, renderer: &mut Render) -> bool;
}

impl<T: Doc + ?Sized> Doc for Box<T> {
    fn render(&self, renderer: &mut Render) -> bool {
        self.as_ref().render(renderer)
    }
}

impl<T: Doc + ?Sized> Doc for Rc<T> {
    fn render(&self, renderer: &mut Render) -> bool {
        self.as_ref().render(renderer)
    }
}

impl<T: Doc + ?Sized> Doc for Arc<T> {
    fn render(&self, renderer: &mut Render) -> bool {
        self.as_ref().render(renderer)
    }
}

pub(crate) type DynDoc = Box<dyn Doc>;

struct Nil;

impl Doc for Nil {
    fn render(&self, renderer: &mut Render) -> bool {
        renderer.nil()
    }
}

pub(crate) struct Text {
    pub(crate) s: String,
}

impl Doc for Text {
    fn render(&self, renderer: &mut Render) -> bool {
        renderer.text(&self.s)
    }
}

struct Concat<A, B> {
    a: A,
    b: B,
}

impl<A: Doc, B: Doc> Doc for Concat<A, B> {
    fn render(&self, renderer: &mut Render) -> bool {
        renderer.concat(|r| self.a.render(r), |r| self.b.render(r))
    }
}

struct Group<D> {
    doc: D,
}

impl<D: Doc> Doc for Group<D> {
    fn render(&self, renderer: &mut Render) -> bool {
        renderer.group(|r| self.doc.render(r))
    }
}

struct Nest<D> {
    indent: usize,
    doc: D,
}

impl<D: Doc> Doc for Nest<D> {
    fn render(&self, renderer: &mut Render) -> bool {
        renderer.nest(self.indent, |r| self.doc.render(r))
    }
}

struct FlatAlt<A, B> {
    flat: A,
    broken: B,
}

impl<A: Doc, B: Doc> Doc for FlatAlt<A, B> {
    fn render(&self, renderer: &mut Render) -> bool {
        renderer.flat_alt(|r| self.flat.render(r), |r| self.broken.render(r))
    }
}

struct Hardline;

impl Doc for Hardline {
    fn render(&self, renderer: &mut Render) -> bool {
        renderer.hardline()
    }
}

struct Fail;

impl Doc for Fail {
    fn render(&self, renderer: &mut Render) -> bool {
        renderer.fail()
    }
}

struct Seq {
    docs: Vec<DynDoc>,
}

impl Doc for Seq {
    fn render(&self, renderer: &mut Render) -> bool {
        for doc in &self.docs {
            if !doc.render(renderer) {
                return false;
            }
        }
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Flat,
    Broken,
}

#[derive(Clone)]
pub(crate) struct Render {
    width: usize,
    output: String,
    col: usize,
    indent: usize,
    mode: Mode,
}

impl Render {
    fn new(width: usize) -> Self {
        Self { width, output: String::new(), col: 0, indent: 0, mode: Mode::Broken }
    }

    fn render<D: Doc>(&mut self, doc: D) -> String {
        let _ = doc.render(self);
        std::mem::take(&mut self.output)
    }

    fn nil(&mut self) -> bool {
        true
    }

    fn text(&mut self, s: &str) -> bool {
        if self.mode == Mode::Flat && s.contains('\n') {
            return false;
        }

        let width = display_width(s);
        if self.mode == Mode::Flat && self.col + width > self.width {
            return false;
        }

        self.output.push_str(s);
        if s.contains('\n') {
            self.col = width;
        } else {
            self.col += width;
        }
        true
    }

    fn concat(
        &mut self,
        left: impl FnOnce(&mut Render) -> bool,
        right: impl FnOnce(&mut Render) -> bool,
    ) -> bool {
        left(self) && right(self)
    }

    fn group(&mut self, doc: impl Fn(&mut Render) -> bool) -> bool {
        let checkpoint = self.clone();
        let outer_mode = self.mode;
        self.mode = Mode::Flat;
        if doc(self) {
            self.mode = outer_mode;
            return true;
        }

        *self = checkpoint;
        if outer_mode == Mode::Flat {
            return false;
        }

        self.mode = Mode::Broken;
        let rendered = doc(self);
        self.mode = outer_mode;
        rendered
    }

    fn nest(&mut self, indent: usize, doc: impl FnOnce(&mut Render) -> bool) -> bool {
        let previous = self.indent;
        self.indent += indent;
        let rendered = doc(self);
        self.indent = previous;
        rendered
    }

    fn flat_alt(
        &mut self,
        flat: impl FnOnce(&mut Render) -> bool,
        broken: impl FnOnce(&mut Render) -> bool,
    ) -> bool {
        if self.mode == Mode::Flat { flat(self) } else { broken(self) }
    }

    fn hardline(&mut self) -> bool {
        if self.mode == Mode::Flat {
            return false;
        }

        self.output.push('\n');
        for _ in 0..self.indent {
            self.output.push(' ');
        }
        self.col = self.indent;
        true
    }

    fn fail(&mut self) -> bool {
        false
    }
}

pub(crate) fn render<D: Doc>(doc: D, width: usize) -> String {
    Render::new(width).render(doc)
}

pub(crate) fn nil() -> impl Doc {
    Nil
}

pub(crate) fn text(s: impl Into<String>) -> impl Doc {
    Text { s: s.into() }
}

pub(crate) fn concat<A: Doc, B: Doc>(a: A, b: B) -> impl Doc {
    Concat { a, b }
}

pub(crate) fn hardline() -> impl Doc {
    Hardline
}

pub(crate) fn group<D: Doc>(doc: D) -> impl Doc {
    Group { doc }
}

pub(crate) fn nest<D: Doc>(indent: usize, doc: D) -> impl Doc {
    Nest { indent, doc }
}

pub(crate) fn flat_alt<A: Doc, B: Doc>(flat: A, broken: B) -> impl Doc {
    FlatAlt { flat, broken }
}

pub(crate) fn fail() -> impl Doc {
    Fail
}

pub(crate) fn space() -> impl Doc {
    text(" ")
}

pub(crate) fn line() -> impl Doc {
    flat_alt(text(" "), hardline())
}

pub(crate) fn softline() -> impl Doc {
    flat_alt(nil(), hardline())
}

pub(crate) fn soft() -> DynDoc {
    boxed(softline())
}

pub(crate) fn boxed<D: Doc + 'static>(doc: D) -> DynDoc {
    Box::new(doc)
}

pub(crate) fn seq(docs: impl IntoIterator<Item = DynDoc>) -> DynDoc {
    boxed(Seq { docs: docs.into_iter().collect() })
}

pub(crate) fn txt(s: impl Into<String>) -> DynDoc {
    boxed(Text { s: s.into() })
}

pub(crate) fn hard() -> DynDoc {
    boxed(hardline())
}

pub(crate) fn join(
    separator: impl Fn() -> DynDoc,
    docs: impl IntoIterator<Item = DynDoc>,
) -> DynDoc {
    let mut result = Vec::new();
    let mut iter = docs.into_iter();

    let Some(first) = iter.next() else {
        return boxed(nil());
    };

    result.push(first);
    for doc in iter {
        result.push(separator());
        result.push(doc);
    }

    seq(result)
}

pub(crate) fn parens(doc: DynDoc) -> DynDoc {
    seq([txt("("), doc, txt(")")])
}

pub(crate) fn brackets(doc: DynDoc) -> DynDoc {
    seq([txt("["), doc, txt("]")])
}

pub(crate) fn braces(doc: DynDoc) -> DynDoc {
    seq([txt("{"), doc, txt("}")])
}

pub(crate) fn comma_sep(docs: impl IntoIterator<Item = DynDoc>) -> DynDoc {
    join(|| seq([txt(","), txt(" ")]), docs)
}

pub(crate) fn comma_line() -> DynDoc {
    seq([txt(","), boxed(line())])
}

pub(crate) trait DocExt: Doc + Sized {
    fn append<D: Doc>(self, other: D) -> impl Doc {
        concat(self, other)
    }

    fn group(self) -> impl Doc {
        group(self)
    }

    fn nest(self, indent: usize) -> impl Doc {
        nest(indent, self)
    }

    fn boxed(self) -> DynDoc
    where
        Self: 'static,
    {
        boxed(self)
    }
}

impl<T: Doc> DocExt for T {}

fn display_width(s: &str) -> usize {
    s.rsplit('\n').next().unwrap_or_default().chars().count()
}
