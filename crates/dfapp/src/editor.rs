//! Equation editor: syntax highlighting with live structural colouring,
//! line gutter with per-equation status, diagnostics underlining, and
//! LSP-style completion / signature help.

use crate::analysis::{Analysis, LineStatus};
use crate::find::{self, Scope};
use dfcore::lexer::{lex, Tok};
use dfcore::{Severity, VarStatus};
use eframe::egui::{self, text::LayoutJob, Color32, FontId, Key, Modifiers, Stroke, TextFormat};
use egui::text::{CCursor, CCursorRange, CharIndex};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

pub const KEYWORDS: &[&str] = &[
    "Duplicate",
    "End",
    "Function",
    "Procedure",
    "Call",
    "If",
    "Then",
    "Else",
    "ElseIf",
    "EndIf",
    "Repeat",
    "Until",
    "While",
    "Do",
    "For",
    "To",
    "Step",
    "and",
    "or",
    "not",
    "Return",
];

pub const DIRECTIVES: &[(&str, &str)] = &[
    ("$UnitSystem", "$UnitSystem Eng | SI | Imperial | SI C bar kJ ..."),
    ("$Guess", "$Guess x = 1, T[3] = 300"),
    ("$Bounds", "$Bounds x = 0 .. 1"),
    ("$Units", "$Units T[*] = C, W_net = MW"),
    ("$Control", "$Control vary x to make y = target"),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Keyword,
    Function,
    Property,
    UserFunction,
    Variable,
    Constant,
    Fluid,
    Directive,
    Unit,
}

#[derive(Clone, Debug)]
pub struct Candidate {
    pub label: String,
    pub insert: String,
    pub kind: Kind,
    pub detail: String,
}

pub struct Completion {
    pub items: Vec<Candidate>,
    pub selected: usize,
    /// char range of the prefix being completed
    pub start: usize,
    pub end: usize,
}

#[derive(Default)]
pub struct Editor {
    pub completion: Option<Completion>,
    hl_cache: Option<(u64, u64, bool, LayoutJob)>,
    /// Request to move the cursor to the start of this 1-based line.
    pub jump_to_line: Option<u32>,
    pub cursor_char: usize,
    pub fluids: Vec<String>,
    pub font_size: f32,
    /// Id of the text widget (for its undo history), set by `show`.
    te_id: Option<egui::Id>,
    /// The editor lost keyboard focus this frame.
    pub lost_focus: bool,
    /// Bracket pairs of the text (cached on its hash).
    pairs: (u64, Option<HashMap<usize, Option<usize>>>),
    /// Find / replace bar (Ctrl+F, Ctrl+H).
    pub find: find::Bar,
    /// Last known selection of the text widget (char range, sorted).
    sel: (usize, usize),
    /// Scroll this char position into view next frame.
    scroll_to: Option<usize>,
}

/// What the find bar asked for this frame.
enum FindAct {
    Next(bool),
    Goto(usize),
    Replace,
    ReplaceAll(Scope),
    Close,
}

struct Palette {
    keyword: Color32,
    function: Color32,
    number: Color32,
    unit: Color32,
    string: Color32,
    comment: Color32,
    directive: Color32,
    text: Color32,
    determined: Color32,
    pending: Color32,
    over: Color32,
    fixed: Color32,
}

fn palette(dark: bool) -> Palette {
    if dark {
        Palette {
            keyword: Color32::from_rgb(198, 120, 221),
            function: Color32::from_rgb(97, 175, 239),
            number: Color32::from_rgb(209, 154, 102),
            unit: Color32::from_rgb(86, 182, 194),
            string: Color32::from_rgb(152, 195, 121),
            comment: Color32::from_rgb(110, 118, 129),
            directive: Color32::from_rgb(229, 192, 123),
            text: Color32::from_rgb(220, 223, 228),
            determined: Color32::from_rgb(120, 200, 120),
            pending: Color32::from_rgb(240, 170, 60),
            over: Color32::from_rgb(240, 90, 90),
            fixed: Color32::from_rgb(120, 170, 255),
        }
    } else {
        Palette {
            keyword: Color32::from_rgb(140, 40, 170),
            function: Color32::from_rgb(20, 90, 190),
            number: Color32::from_rgb(170, 90, 0),
            unit: Color32::from_rgb(0, 130, 140),
            string: Color32::from_rgb(60, 130, 30),
            comment: Color32::from_rgb(120, 120, 120),
            directive: Color32::from_rgb(160, 110, 0),
            text: Color32::from_rgb(30, 30, 30),
            determined: Color32::from_rgb(20, 130, 40),
            pending: Color32::from_rgb(200, 110, 0),
            over: Color32::from_rgb(200, 30, 30),
            fixed: Color32::from_rgb(30, 90, 200),
        }
    }
}

pub fn status_color(s: VarStatus, dark: bool) -> Color32 {
    let p = palette(dark);
    match s {
        VarStatus::Determined => p.determined,
        VarStatus::Fixed => p.fixed,
        VarStatus::UnderDetermined => p.pending,
        VarStatus::OverDetermined => p.over,
        VarStatus::Unused => p.comment,
    }
}

pub fn line_color(s: LineStatus, dark: bool) -> Color32 {
    let p = palette(dark);
    match s {
        LineStatus::Determined => p.determined,
        LineStatus::Pending => p.pending,
        LineStatus::Over | LineStatus::Error | LineStatus::Failed => p.over,
    }
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '#' || c == '$'
}

/// Build the highlighted layout.
fn highlight(text: &str, an: Option<&Analysis>, focus: Option<&Focus>, dark: bool, font: FontId) -> LayoutJob {
    let p = palette(dark);
    // byte offset of the start of each line, for the focus dimming
    let line_starts: Vec<usize> = std::iter::once(0).chain(text.match_indices('\n').map(|(i, _)| i + 1)).collect();
    let line_of = |b: usize| line_starts.partition_point(|&s| s <= b) as u32;
    let mark = if dark { Color32::from_rgba_unmultiplied(250, 200, 60, 70) } else { Color32::from_rgba_unmultiplied(250, 190, 40, 110) };
    let lexed = lex(text);
    let mut job = LayoutJob::default();
    // segments: (start, end, color, italic, focus mark)
    let mut segs: Vec<(usize, usize, Color32, bool, bool)> = Vec::with_capacity(lexed.tokens.len() + lexed.comments.len());
    let toks = &lexed.tokens;
    for (i, t) in toks.iter().enumerate() {
        let marked = match (&t.tok, focus) {
            (Tok::Ident(s), Some(f)) if s.eq_ignore_ascii_case(&f.base) => match (&f.index, toks.get(i + 1).map(|t| &t.tok), toks.get(i + 2).map(|t| &t.tok)) {
                // `T[3]` is only the focused element when the literal index matches
                (Some(k), Some(Tok::LBracket), Some(Tok::Num(n))) => (*n - *k as f64).abs() < 0.5,
                (None, Some(Tok::LBracket), _) => false,
                _ => true,
            },
            _ => false,
        };
        let col = match &t.tok {
            Tok::Num(_) => p.number,
            Tok::Unit(_) => p.unit,
            Tok::Str(_) => p.string,
            Tok::Directive(..) => p.directive,
            Tok::Ident(s) => {
                let l = s.to_lowercase();
                let next_paren = matches!(toks.get(i + 1).map(|t| &t.tok), Some(Tok::LParen));
                if KEYWORDS.iter().any(|k| k.eq_ignore_ascii_case(&l)) {
                    p.keyword
                } else if next_paren {
                    p.function
                } else if let Some(st) = an.and_then(|a| a.name_status.get(&l)) {
                    status_color(*st, dark)
                } else {
                    p.text
                }
            }
            Tok::Newline | Tok::Eof => continue,
            _ => p.text,
        };
        segs.push((t.span.start, t.span.end, col, false, marked));
    }
    for c in &lexed.comments {
        segs.push((c.span.start, c.span.end, p.comment, true, false));
    }
    segs.sort_by_key(|s| s.0);
    let errs: Vec<(usize, usize)> =
        an.map(|a| a.model.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| (d.span.start, d.span.end.max(d.span.start + 1))).collect()).unwrap_or_default();
    let fmt = |c: Color32, italic: bool, marked: bool, s: usize, e: usize| {
        let bad = errs.iter().any(|(a, b)| s < *b && e > *a);
        // lines that do not mention the focused variable are dimmed
        let dim = focus.is_some_and(|f| !f.lines.contains(&line_of(s)));
        TextFormat {
            font_id: font.clone(),
            color: if dim { c.gamma_multiply(0.3) } else { c },
            italics: italic,
            background: if marked { mark } else { Color32::TRANSPARENT },
            underline: if bad { Stroke::new(1.5, p.over) } else { Stroke::NONE },
            ..Default::default()
        }
    };
    // plain text between segments, split at line breaks so that dimming is per line
    let plain = |job: &mut LayoutJob, a: usize, b: usize| {
        let mut x = a;
        while x < b {
            let y = text[x..b].find('\n').map(|k| x + k + 1).unwrap_or(b);
            job.append(&text[x..y], 0.0, fmt(p.text, false, false, x, y));
            x = y;
        }
    };
    let mut pos = 0;
    for (s, e, c, it, mk) in segs {
        if s < pos || e > text.len() || !text.is_char_boundary(s) || !text.is_char_boundary(e) {
            continue;
        }
        if s > pos {
            plain(&mut job, pos, s);
        }
        job.append(&text[s..e], 0.0, fmt(c, it, mk, s, e));
        pos = e;
    }
    if pos < text.len() {
        plain(&mut job, pos, text.len());
    }
    job
}

pub struct EditorContext<'a> {
    pub analysis: Option<&'a Analysis>,
    /// Variable under study: its lines stay lit, the rest are dimmed.
    pub focus: Option<&'a Focus>,
}

/// Highlight request for one variable.
#[derive(Clone, Debug, PartialEq)]
pub struct Focus {
    /// Canonical array base (or scalar name), lower case.
    pub base: String,
    /// Element index for a one-dimensional array element.
    pub index: Option<i64>,
    /// 1-based lines of the equations that contain the variable.
    pub lines: std::collections::HashSet<u32>,
}

impl Focus {
    fn key(&self) -> u64 {
        let mut h = DefaultHasher::new();
        self.base.hash(&mut h);
        self.index.hash(&mut h);
        let mut l: Vec<_> = self.lines.iter().collect();
        l.sort();
        l.hash(&mut h);
        h.finish()
    }
}

impl Editor {
    /// Undo (`redo = false`) or redo the last edit through the text widget's
    /// own history. Returns true when the text changed.
    pub fn history(&mut self, ctx: &egui::Context, text: &mut String, redo: bool) -> bool {
        let Some(id) = self.te_id else { return false };
        let Some(mut st) = egui::TextEdit::load_state(ctx, id) else { return false };
        let mut u = st.undoer();
        let cur = (st.cursor.char_range().unwrap_or(CCursorRange::one(CCursor::new(CharIndex(0)))), text.clone());
        let r = if redo { u.redo(&cur).cloned() } else { u.undo(&cur).cloned() };
        match r {
            Some((range, t)) => {
                *text = t;
                st.cursor.set_char_range(Some(range));
                st.set_undoer(u);
                st.store(ctx, id);
                true
            }
            None => false,
        }
    }

    /// Record the current text as an undo point before a programmatic change
    /// (formatting), so that Undo restores it.
    pub fn checkpoint(&mut self, ctx: &egui::Context, text: &str) {
        let Some(id) = self.te_id else { return };
        let Some(mut st) = egui::TextEdit::load_state(ctx, id) else { return };
        let mut u = st.undoer();
        u.add_undo(&(st.cursor.char_range().unwrap_or(CCursorRange::one(CCursor::new(CharIndex(0)))), text.to_string()));
        st.set_undoer(u);
        st.store(ctx, id);
    }

    /// Open the find bar (`replace`: with the replace row). A one-line
    /// selection becomes the query; a selection over several lines becomes
    /// the search range.
    pub fn open_find(&mut self, replace: bool, text: &str) {
        let f = &mut self.find;
        f.open = true;
        f.replace = replace;
        f.focus = true;
        let (a, b) = self.sel;
        if a < b {
            let s: String = text.chars().skip(a).take(b - a).collect();
            if s.contains('\n') {
                f.in_sel = Some((a, b));
            } else {
                f.query = s;
            }
        }
    }

    /// Select `[a, b)` in the text widget and scroll to it.
    fn select(&mut self, ctx: &egui::Context, a: usize, b: usize) {
        self.sel = (a, b);
        self.scroll_to = Some(a);
        let Some(id) = self.te_id else { return };
        if let Some(mut st) = egui::TextEdit::load_state(ctx, id) {
            st.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(CharIndex(a)), CCursor::new(CharIndex(b)))));
            st.store(ctx, id);
        }
    }

    /// Matches of the query inside the search range.
    fn find_matches(&self, text: &str) -> Vec<(usize, usize)> {
        let f = &self.find;
        find::matches(text, &f.query, f.opts).into_iter().filter(|m| f.in_sel.is_none_or(|(a, b)| m.0 >= a && m.1 <= b)).collect()
    }

    /// The find / replace bar; returns true when the text changed.
    fn find_bar(&mut self, ui: &mut egui::Ui, text: &mut String) -> bool {
        let (cf, ch, f3, sf3) = ui.input_mut(|i| {
            (
                i.consume_key(Modifiers::COMMAND, Key::F),
                i.consume_key(Modifiers::COMMAND, Key::H),
                i.consume_key(Modifiers::NONE, Key::F3),
                i.consume_key(Modifiers::SHIFT, Key::F3),
            )
        });
        if cf || ch {
            self.open_find(ch, text);
        }
        if !self.find.open {
            return false;
        }
        let ctx = ui.ctx().clone();
        let ms = self.find_matches(text);
        let cur = ms.iter().position(|m| *m == self.sel);
        let mut act = if f3 {
            Some(FindAct::Next(true))
        } else if sf3 {
            Some(FindAct::Next(false))
        } else {
            None
        };
        let mut focused = false;
        let enter = ui.input(|i| i.key_pressed(Key::Enter));
        let shift = ui.input(|i| i.modifiers.shift);
        let sel = self.sel;
        let f = &mut self.find;
        egui::Frame::group(ui.style()).inner_margin(4.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                let arrow = if f.replace { "−" } else { "+" };
                if ui.small_button(arrow).on_hover_text("Show or hide replace (Ctrl+H)").clicked() {
                    f.replace = !f.replace;
                }
                let q0 = f.query.clone();
                let r = ui.add(egui::TextEdit::singleline(&mut f.query).hint_text("Find").desired_width(220.0).font(egui::TextStyle::Monospace));
                if f.focus {
                    r.request_focus();
                    f.focus = false;
                }
                if f.query != q0 {
                    f.note.clear();
                }
                focused |= r.has_focus();
                if (r.lost_focus() || r.has_focus()) && enter {
                    act = Some(FindAct::Next(!shift));
                    r.request_focus();
                }
                ui.toggle_value(&mut f.opts.case, "Aa").on_hover_text("Match case");
                ui.toggle_value(&mut f.opts.word, "ab|").on_hover_text("Whole word");
                let mut insel = f.in_sel.is_some();
                if ui.toggle_value(&mut insel, "In selection").on_hover_text("Find and replace only inside the text selected when this is switched on").changed() {
                    f.in_sel = if insel && sel.0 < sel.1 { Some(sel) } else { None };
                }
                let count = if !f.note.is_empty() {
                    f.note.clone()
                } else if f.query.is_empty() {
                    String::new()
                } else {
                    match (cur, ms.len()) {
                        (_, 0) => "No results".into(),
                        (Some(k), n) => format!("{} of {n}", k + 1),
                        (None, n) => format!("{n} found"),
                    }
                };
                ui.label(count);
                if ui.small_button("Prev").on_hover_text("Previous (Shift+Enter, Shift+F3)").clicked() {
                    act = Some(FindAct::Next(false));
                }
                if ui.small_button("Next").on_hover_text("Next (Enter, F3)").clicked() {
                    act = Some(FindAct::Next(true));
                }
                ui.toggle_value(&mut f.list, "Find all").on_hover_text("List every match; click one to go to it");
                if ui.small_button("×").on_hover_text("Close (Esc)").clicked() {
                    act = Some(FindAct::Close);
                }
            });
            if f.replace {
                ui.horizontal(|ui| {
                    ui.add_space(ui.spacing().interact_size.y + ui.spacing().item_spacing.x);
                    let r = ui.add(egui::TextEdit::singleline(&mut f.with).hint_text("Replace with").desired_width(220.0).font(egui::TextStyle::Monospace));
                    focused |= r.has_focus();
                    if (r.lost_focus() || r.has_focus()) && enter {
                        act = Some(FindAct::Replace);
                        r.request_focus();
                    }
                    if ui.button("Replace").on_hover_text("Replace the selected match and go to the next one (Enter)").clicked() {
                        act = Some(FindAct::Replace);
                    }
                    let all = if f.in_sel.is_some() { "Replace all in selection" } else { "Replace all" };
                    if ui.button(all).clicked() {
                        act = Some(FindAct::ReplaceAll(Scope::All));
                    }
                    if ui.button("Before cursor").on_hover_text("Replace every match that ends before the cursor (or the start of the selection)").clicked() {
                        act = Some(FindAct::ReplaceAll(Scope::Before(sel.0)));
                    }
                    if ui.button("After cursor").on_hover_text("Replace every match from the cursor (or the start of the selection) on").clicked() {
                        act = Some(FindAct::ReplaceAll(Scope::After(sel.0)));
                    }
                });
            }
            if f.list && !ms.is_empty() {
                // char index of each line start
                let mut starts = vec![0usize];
                starts.extend(text.chars().enumerate().filter(|(_, c)| *c == '\n').map(|(i, _)| i + 1));
                let lines: Vec<&str> = text.split('\n').collect();
                egui::ScrollArea::vertical().id_salt("find_all").max_height(150.0).show(ui, |ui| {
                    for (k, m) in ms.iter().enumerate().take(2000) {
                        let ln = starts.partition_point(|s| *s <= m.0);
                        let label = format!("{ln:>5}  {}", lines.get(ln - 1).map(|l| l.trim()).unwrap_or(""));
                        if ui.selectable_label(cur == Some(k), egui::RichText::new(label).monospace()).clicked() {
                            act = Some(FindAct::Goto(k));
                        }
                    }
                });
            }
        });
        if focused && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            act = Some(FindAct::Close);
        }
        let (q, with, opts) = (self.find.query.clone(), self.find.with.clone(), self.find.opts);
        let (m, r) = (q.chars().count(), with.chars().count());
        match act {
            Some(FindAct::Next(fwd)) => {
                self.find.note.clear();
                if let Some(k) = find::next(&ms, self.sel, fwd) {
                    self.select(&ctx, ms[k].0, ms[k].1);
                }
                false
            }
            Some(FindAct::Goto(k)) => {
                self.select(&ctx, ms[k].0, ms[k].1);
                false
            }
            Some(FindAct::Replace) => {
                self.find.note.clear();
                let Some(k) = cur else {
                    // nothing selected yet: go to the next match first
                    if let Some(k) = find::next(&ms, self.sel, true) {
                        self.select(&ctx, ms[k].0, ms[k].1);
                    }
                    return false;
                };
                let (a, b) = ms[k];
                self.checkpoint(&ctx, text);
                text.replace_range(char_to_byte(text, a)..char_to_byte(text, b), &with);
                if let Some((x, y)) = self.find.in_sel {
                    self.find.in_sel = Some((x, (y + r).saturating_sub(m)));
                }
                let ms2 = self.find_matches(text);
                match find::next(&ms2, (a + r, a + r), true) {
                    Some(k) => self.select(&ctx, ms2[k].0, ms2[k].1),
                    None => self.select(&ctx, a + r, a + r),
                }
                true
            }
            Some(FindAct::ReplaceAll(scope)) => {
                let scope = match (scope, self.find.in_sel) {
                    (Scope::All, Some((a, b))) => Scope::Range(a, b),
                    (Scope::Before(c), Some((a, b))) => Scope::Range(a, b.min(c)),
                    (Scope::After(c), Some((a, b))) => Scope::Range(a.max(c), b),
                    (s, _) => s,
                };
                let (new, rep) = find::replace(text, &q, &with, opts, scope);
                self.find.note = format!("Replaced {}", rep.len());
                if rep.is_empty() {
                    return false;
                }
                self.checkpoint(&ctx, text);
                *text = new;
                let c = find::shift(self.sel.0, &rep, m, r);
                if let Some((x, y)) = self.find.in_sel {
                    self.find.in_sel = Some((find::shift(x, &rep, m, r), find::shift(y, &rep, m, r)));
                }
                self.select(&ctx, c, c);
                self.scroll_to = None;
                true
            }
            Some(FindAct::Close) => {
                self.find.open = false;
                self.find.list = false;
                self.find.note.clear();
                if let Some(id) = self.te_id {
                    ctx.memory_mut(|mm| mm.request_focus(id));
                }
                false
            }
            None => false,
        }
    }

    pub fn new() -> Self {
        Editor { font_size: 14.0, ..Default::default() }
    }

    /// Show the editor; returns true when the text changed.
    pub fn show(&mut self, ui: &mut egui::Ui, text: &mut String, ectx: &EditorContext) -> bool {
        let dark = ui.visuals().dark_mode;
        let id = ui.make_persistent_id("equation_editor");
        let font = FontId::monospace(self.font_size);
        let gen = ectx.analysis.map(|a| a.gen).unwrap_or(0);

        // ---- completion keys must be consumed before the TextEdit sees them
        let mut accept: Option<Candidate> = None;
        if let Some(c) = &mut self.completion {
            let n = c.items.len();
            ui.input_mut(|i| {
                if i.consume_key(Modifiers::NONE, Key::ArrowDown) {
                    c.selected = (c.selected + 1) % n;
                }
                if i.consume_key(Modifiers::NONE, Key::ArrowUp) {
                    c.selected = (c.selected + n - 1) % n;
                }
                if i.consume_key(Modifiers::NONE, Key::Enter) || i.consume_key(Modifiers::NONE, Key::Tab) {
                    accept = Some(c.items[c.selected].clone());
                }
            });
            if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
                self.completion = None;
            }
        }
        let force_complete = ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Space));

        let mut changed = false;
        if let Some(cand) = accept {
            let c = self.completion.take().unwrap();
            let (bs, be) = (char_to_byte(text, c.start), char_to_byte(text, c.end));
            text.replace_range(bs..be, &cand.insert);
            let new_cursor = c.start + cand.insert.chars().count();
            if let Some(mut st) = egui::TextEdit::load_state(ui.ctx(), id) {
                st.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(CharIndex(new_cursor)))));
                st.store(ui.ctx(), id);
            }
            changed = true;
        }

        changed |= self.find_bar(ui, text);

        // ---- highlighted layouter (cached on text hash + analysis generation)
        let cache = &mut self.hl_cache;
        let fsz = self.font_size;
        let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
            let s = buf.as_str();
            let mut hasher = DefaultHasher::new();
            s.hash(&mut hasher);
            fsz.to_bits().hash(&mut hasher);
            ectx.focus.map(|f| f.key()).hash(&mut hasher);
            let h = hasher.finish();
            let job = match cache {
                Some((hh, g, d, job)) if *hh == h && *g == gen && *d == dark => job.clone(),
                _ => {
                    let job = highlight(s, ectx.analysis, ectx.focus, dark, FontId::monospace(fsz));
                    *cache = Some((h, gen, dark, job.clone()));
                    job
                }
            };
            let mut job = job;
            job.wrap.max_width = wrap;
            ui.fonts_mut(|f| f.layout_job(job))
        };

        let gutter_w = (self.font_size * 3.6).max(36.0);
        let mut out_opt = None;
        egui::ScrollArea::both().id_salt("editor_scroll").auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal_top(|ui| {
                let (gutter_rect, _) = ui.allocate_exact_size(egui::vec2(gutter_w, ui.available_height().max(10.0)), egui::Sense::hover());
                let te =
                    egui::TextEdit::multiline(text).id(id).font(font.clone()).code_editor().desired_width(f32::INFINITY).desired_rows(40).lock_focus(true).layouter(&mut layouter);
                let out = te.show(ui);
                self.te_id = Some(id);
                // moving to the find bar is not leaving the editor (no format on blur)
                self.lost_focus = out.response.lost_focus() && !self.find.open;
                // egui collapses the selection of an unfocused text widget, so
                // the selection is taken only while the editor has the focus
                // (the find bar keeps its own while it is in use)
                if let Some(r) = out.state.cursor.char_range().filter(|_| out.response.has_focus()) {
                    let (p, q) = (r.primary.index.0, r.secondary.index.0);
                    self.sel = (p.min(q), p.max(q));
                }
                if let Some(k) = self.scroll_to.take() {
                    let r = out.galley.pos_from_cursor(CCursor::new(CharIndex(k)));
                    ui.scroll_to_rect(r.translate(out.galley_pos.to_vec2()).expand(40.0), Some(egui::Align::Center));
                }
                // ---- find: mark every match, the selected one stronger
                if self.find.open && !self.find.query.is_empty() {
                    let (fill, strong) = if dark {
                        (Color32::from_rgba_unmultiplied(250, 200, 60, 45), Color32::from_rgb(250, 200, 60))
                    } else {
                        (Color32::from_rgba_unmultiplied(250, 180, 0, 70), Color32::from_rgb(210, 140, 0))
                    };
                    let clip = ui.clip_rect();
                    let p = ui.painter();
                    let ms = find::matches(text, &self.find.query, self.find.opts);
                    for (a, b) in ms.into_iter().filter(|m| self.find.in_sel.is_none_or(|(x, y)| m.0 >= x && m.1 <= y)) {
                        let r0 = out.galley.pos_from_cursor(CCursor::new(CharIndex(a)));
                        let r1 = out.galley.pos_from_cursor(CCursor::new(CharIndex(b)));
                        let x1 = if (r1.min.y - r0.min.y).abs() < 1.0 { r1.min.x } else { r0.min.x + self.font_size * 0.6 * (b - a) as f32 };
                        let rect = egui::Rect::from_min_max(r0.min, egui::pos2(x1, r0.max.y)).translate(out.galley_pos.to_vec2());
                        if !clip.intersects(rect) {
                            continue;
                        }
                        p.rect_filled(rect, 2.0, fill);
                        if (a, b) == self.sel {
                            p.rect_stroke(rect, 2.0, egui::Stroke::new(1.5, strong), egui::StrokeKind::Outside);
                        }
                    }
                }
                // lines of the search range, marked in the gutter
                let sel_lines = self.find.in_sel.filter(|_| self.find.open).map(|(a, b)| {
                    let line = |c: usize| 1 + text.chars().take(c).filter(|ch| *ch == '\n').count() as u32;
                    (line(a), line(b.saturating_sub(1).max(a)))
                });
                // ---- gutter: line numbers coloured by equation status
                let painter = ui.painter_at(gutter_rect.expand2(egui::vec2(0.0, 1e6)));
                let mut line: u32 = 1;
                let mut new_line = true;
                let small = FontId::monospace(self.font_size * 0.85);
                for row in &out.galley.rows {
                    if new_line {
                        let y = out.galley_pos.y + row.rect().min.y;
                        let st = ectx.analysis.and_then(|a| a.line_status.get(&line).copied());
                        let col = match st {
                            Some(s) => line_color(s, dark),
                            None => ui.visuals().weak_text_color(),
                        };
                        if st.is_some() {
                            painter.rect_filled(egui::Rect::from_min_size(egui::pos2(gutter_rect.max.x - 5.0, y + 1.0), egui::vec2(3.0, row.rect().height() - 2.0)), 1.0, col);
                        }
                        painter.text(egui::pos2(gutter_rect.max.x - 9.0, y), egui::Align2::RIGHT_TOP, line.to_string(), small.clone(), col);
                        if sel_lines.is_some_and(|(a, b)| line >= a && line <= b) {
                            let c = if dark { Color32::from_rgb(120, 170, 255) } else { Color32::from_rgb(0, 90, 220) };
                            painter.rect_filled(egui::Rect::from_min_size(egui::pos2(gutter_rect.min.x + 1.0, y), egui::vec2(3.0, row.rect().height())), 0.0, c);
                        }
                    }
                    new_line = row.ends_with_newline;
                    if row.ends_with_newline {
                        line += 1;
                    }
                }
                // ---- jump to line
                if let Some(l) = self.jump_to_line.take() {
                    let idx = text.split('\n').take((l as usize).saturating_sub(1)).map(|s| s.chars().count() + 1).sum::<usize>();
                    let mut st = out.state.clone();
                    st.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(CharIndex(idx)))));
                    st.store(ui.ctx(), id);
                    let r = out.galley.pos_from_cursor(CCursor::new(CharIndex(idx)));
                    ui.scroll_to_rect(r.translate(out.galley_pos.to_vec2()).expand(40.0), Some(egui::Align::Center));
                    out.response.request_focus();
                }
                // ---- matching bracket: the one under the mouse, else the
                // one next to the text cursor
                let h = {
                    let mut hs = DefaultHasher::new();
                    text.hash(&mut hs);
                    hs.finish()
                };
                if self.pairs.0 != h || self.pairs.1.is_none() {
                    self.pairs = (h, Some(bracket_pairs(text)));
                }
                let pairs = self.pairs.1.as_ref().unwrap();
                // the char under the pointer (cursor_from_pos gives the nearest gap)
                let hover = ui.input(|i| i.pointer.hover_pos()).filter(|p| out.response.rect.contains(*p)).map(|p| {
                    let k = out.galley.cursor_from_pos(p - out.galley_pos).index.0;
                    let x = out.galley_pos.x + out.galley.pos_from_cursor(CCursor::new(CharIndex(k))).min.x;
                    if p.x < x {
                        k.saturating_sub(1)
                    } else {
                        k
                    }
                });
                // caret: the bracket after it, else the one before it
                let near = |k: usize| -> Option<usize> { [k, k.wrapping_sub(1)].into_iter().find(|j| pairs.contains_key(j)) };
                let at = match hover {
                    Some(k) if pairs.contains_key(&k) => Some(k),
                    _ => out.cursor_range.filter(|_| out.response.has_focus()).and_then(|r| near(r.primary.index.0)),
                };
                if let Some(a) = at {
                    let rect_of = |k: usize| {
                        let r0 = out.galley.pos_from_cursor(CCursor::new(CharIndex(k)));
                        let r1 = out.galley.pos_from_cursor(CCursor::new(CharIndex(k + 1)));
                        let w = if (r1.min.y - r0.min.y).abs() < 1.0 { (r1.min.x - r0.min.x).max(2.0) } else { self.font_size * 0.6 };
                        egui::Rect::from_min_size(r0.min, egui::vec2(w, r0.height())).translate(out.galley_pos.to_vec2())
                    };
                    let p = ui.painter();
                    match pairs.get(&a).copied().flatten() {
                        Some(b) => {
                            let (fill, stroke) = if dark {
                                (Color32::from_rgba_unmultiplied(120, 170, 255, 40), Color32::from_rgb(120, 170, 255))
                            } else {
                                (Color32::from_rgba_unmultiplied(0, 90, 220, 28), Color32::from_rgb(0, 90, 220))
                            };
                            for k in [a, b] {
                                let r = rect_of(k);
                                p.rect_filled(r, 2.0, fill);
                                p.rect_stroke(r, 2.0, egui::Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
                            }
                        }
                        None => {
                            let r = rect_of(a);
                            p.rect_filled(r, 2.0, Color32::from_rgba_unmultiplied(220, 40, 40, 50));
                            p.rect_stroke(r, 2.0, egui::Stroke::new(1.0, Color32::from_rgb(220, 40, 40)), egui::StrokeKind::Inside);
                        }
                    }
                }
                out_opt = Some(out);
            });
        });
        let Some(out) = out_opt else { return changed };
        let response = &out.response;
        changed |= response.changed();
        let cursor = out.cursor_range.map(|r| r.primary.index.0);
        if let Some(c) = cursor {
            self.cursor_char = c;
        }

        // ---- completion popup management
        if let Some(cur) = cursor {
            let typed = response.changed();
            let (start, prefix) = word_before(text, cur);
            let wants = force_complete || (typed && !prefix.is_empty() && !prefix.chars().next().unwrap().is_ascii_digit());
            if wants || (self.completion.is_some() && typed) {
                let items = candidates(text, start, &prefix, ectx, &self.fluids);
                if items.is_empty() || (items.len() == 1 && items[0].insert.eq_ignore_ascii_case(&prefix) && !force_complete) {
                    self.completion = None;
                } else {
                    self.completion = Some(Completion { items, selected: 0, start, end: cur });
                }
            } else if let Some(c) = &self.completion {
                if cur != c.end {
                    self.completion = None;
                }
            }
        }
        if !response.has_focus() && !response.lost_focus() {
            // keep popup only while editing
        }
        // ---- draw popup
        if let (Some(c), Some(cur)) = (&mut self.completion, cursor) {
            let r = out.galley.pos_from_cursor(CCursor::new(CharIndex(cur)));
            let pos = out.galley_pos + r.left_bottom().to_vec2() + egui::vec2(0.0, 2.0);
            let mut clicked: Option<usize> = None;
            egui::Area::new(id.with("completion")).order(egui::Order::Foreground).fixed_pos(pos).show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_max_width(460.0);
                    egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                        for (i, it) in c.items.iter().enumerate().take(200) {
                            let sel = i == c.selected;
                            let kind = match it.kind {
                                Kind::Keyword => "kw",
                                Kind::Function => "fn",
                                Kind::Property => "prop",
                                Kind::UserFunction => "user",
                                Kind::Variable => "var",
                                Kind::Constant => "const",
                                Kind::Fluid => "fluid",
                                Kind::Directive => "dir",
                                Kind::Unit => "unit",
                            };
                            let resp = ui.horizontal(|ui| {
                                ui.add_sized([38.0, 16.0], egui::Label::new(egui::RichText::new(kind).small().weak()));
                                let r = ui.selectable_label(sel, egui::RichText::new(&it.label).monospace());
                                if sel {
                                    r.scroll_to_me(None);
                                }
                                ui.label(egui::RichText::new(&it.detail).small().weak());
                                r
                            });
                            if resp.inner.clicked() {
                                clicked = Some(i);
                            }
                        }
                    });
                });
            });
            if let Some(i) = clicked {
                let cand = c.items[i].clone();
                let (bs, be) = (char_to_byte(text, c.start), char_to_byte(text, c.end));
                text.replace_range(bs..be, &cand.insert);
                let nc = c.start + cand.insert.chars().count();
                let mut st = out.state.clone();
                st.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(CharIndex(nc)))));
                st.store(ui.ctx(), id);
                self.completion = None;
                changed = true;
            }
        }
        // ---- signature help
        if self.completion.is_none() && response.has_focus() {
            if let Some(cur) = cursor {
                if let Some(sig) = signature_at(text, cur, ectx) {
                    let r = out.galley.pos_from_cursor(CCursor::new(CharIndex(cur)));
                    let pos = out.galley_pos + r.left_top().to_vec2() - egui::vec2(0.0, 22.0);
                    egui::Area::new(id.with("sig")).order(egui::Order::Foreground).fixed_pos(pos).interactable(false).show(ui.ctx(), |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            ui.label(egui::RichText::new(sig).monospace().small());
                        });
                    });
                }
            }
        }
        changed
    }
}

/// Bracket pairs of a script, by char index: `(` `)`, `[` `]` in code, and
/// the (nestable) `{` `}` of brace comments. Brackets inside `"..."`, `//`
/// and brace comments are not code; a `"` inside a brace comment is plain
/// text. Unmatched brackets map to `None`.
pub fn bracket_pairs(text: &str) -> HashMap<usize, Option<usize>> {
    let c: Vec<char> = text.chars().collect();
    let mut pairs = HashMap::new();
    let mut code: Vec<(char, usize)> = Vec::new();
    let mut braces: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if !braces.is_empty() {
            // inside a brace comment: only braces count
            match ch {
                '{' => braces.push(i),
                '}' => {
                    let o = braces.pop().unwrap();
                    pairs.insert(o, Some(i));
                    pairs.insert(i, Some(o));
                }
                _ => {}
            }
            i += 1;
            continue;
        }
        match ch {
            '"' => {
                i += 1;
                while i < c.len() && c[i] != '"' {
                    i += 1;
                }
            }
            '/' if c.get(i + 1) == Some(&'/') => {
                while i < c.len() && c[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            '{' => braces.push(i),
            '}' => {
                pairs.insert(i, None);
            }
            '(' | '[' => code.push((ch, i)),
            ')' | ']' => {
                let want = if ch == ')' { '(' } else { '[' };
                match code.last() {
                    Some(&(o, k)) if o == want => {
                        code.pop();
                        pairs.insert(k, Some(i));
                        pairs.insert(i, Some(k));
                    }
                    _ => {
                        pairs.insert(i, None);
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    for (_, k) in code {
        pairs.insert(k, None);
    }
    for k in braces {
        pairs.insert(k, None);
    }
    pairs
}

fn char_to_byte(s: &str, c: usize) -> usize {
    s.char_indices().nth(c).map(|(b, _)| b).unwrap_or(s.len())
}

/// (char start, prefix) of the identifier ending at `cur`.
fn word_before(text: &str, cur: usize) -> (usize, String) {
    let chars: Vec<char> = text.chars().take(cur).collect();
    let mut i = chars.len();
    while i > 0 && is_ident_char(chars[i - 1]) {
        i -= 1;
    }
    (i, chars[i..].iter().collect())
}

fn context_before(text: &str, start: usize) -> String {
    let chars: Vec<char> = text.chars().take(start).collect();
    let from = chars.iter().rposition(|c| *c == '\n').map(|p| p + 1).unwrap_or(0);
    chars[from..].iter().collect()
}

fn candidates(text: &str, start: usize, prefix: &str, ectx: &EditorContext, fluids: &[String]) -> Vec<Candidate> {
    let pl = prefix.to_lowercase();
    let mut out: Vec<Candidate> = Vec::new();
    let before = context_before(text, start);
    let trimmed = before.trim_end();
    let matches = |s: &str| pl.is_empty() || s.to_lowercase().starts_with(&pl);
    // unit context: "12 [" ...
    if let Some(open) = before.rfind('[') {
        if !before[open..].contains(']') && before[..open].trim_end().chars().last().map(|c| c.is_ascii_digit() || c == '.').unwrap_or(false) {
            for u in [
                "C", "K", "F", "bar", "kPa", "MPa", "Pa", "atm", "psia", "kJ/kg", "J/kg", "kJ/kg-K", "kW", "MW", "W", "kg/s", "kg/h", "t/h", "m^3/kg", "kg/m^3", "m/s", "m", "mm",
                "s", "h", "-", "%",
            ] {
                if matches(u) {
                    out.push(Candidate { label: u.into(), insert: u.into(), kind: Kind::Unit, detail: String::new() });
                }
            }
            return out;
        }
    }
    // fluid context: first argument of a property function
    let fluid_ctx = {
        let t = trimmed.to_lowercase();
        t.ends_with('(') && {
            let name: String = t[..t.len() - 1].chars().rev().take_while(|c| is_ident_char(*c)).collect::<String>().chars().rev().collect();
            dfcore::props::property_function(&name).is_some() || name == "t_sat" || name == "p_sat"
        }
    };
    if fluid_ctx {
        for f in fluids {
            if matches(f) {
                out.push(Candidate { label: f.clone(), insert: f.clone(), kind: Kind::Fluid, detail: "CoolProp fluid".into() });
            }
        }
        if !out.is_empty() {
            return out;
        }
    }
    if prefix.starts_with('$') {
        for (d, doc) in DIRECTIVES {
            if d.to_lowercase().starts_with(&pl) {
                out.push(Candidate { label: d.to_string(), insert: format!("{d} "), kind: Kind::Directive, detail: doc.to_string() });
            }
        }
        return out;
    }
    // named property inputs after a fluid argument
    let in_prop_call = signature_at(text, start, ectx).map(|s| s.contains("Fluid")).unwrap_or(false);
    if in_prop_call && (trimmed.ends_with(',') || trimmed.ends_with(';')) {
        for (k, d) in [
            ("P", "pressure"),
            ("T", "temperature"),
            ("h", "specific enthalpy"),
            ("s", "specific entropy"),
            ("x", "quality"),
            ("v", "specific volume"),
            ("d", "density"),
            ("u", "internal energy"),
        ] {
            if matches(k) {
                out.push(Candidate { label: format!("{k}="), insert: format!("{k}="), kind: Kind::Keyword, detail: d.into() });
            }
        }
    }
    for k in KEYWORDS {
        if matches(k) && pl.len() >= 2 {
            out.push(Candidate { label: k.to_string(), insert: k.to_string(), kind: Kind::Keyword, detail: String::new() });
        }
    }
    for (f, doc) in dfcore::compile::MATH_FUNCTIONS {
        if matches(f) {
            out.push(Candidate { label: f.to_string(), insert: format!("{f}("), kind: Kind::Function, detail: doc.to_string() });
        }
    }
    for (f, doc) in dfcore::props::PROPERTY_FUNCTIONS {
        if matches(f) {
            let ins = match *f {
                "propssi" => "PropsSI(".to_string(),
                "props1si" => "Props1SI(".to_string(),
                "hapropssi" => "HAPropsSI(".to_string(),
                _ => format!("{f}("),
            };
            out.push(Candidate { label: ins.trim_end_matches('(').to_string(), insert: ins, kind: Kind::Property, detail: doc.to_string() });
        }
    }
    for (c, doc) in dfcore::compile::BUILTIN_CONSTS {
        if matches(c) {
            out.push(Candidate { label: c.to_string(), insert: c.to_string(), kind: Kind::Constant, detail: doc.to_string() });
        }
    }
    if let Some(a) = ectx.analysis {
        for cell in &a.model.funcs {
            if let Some(src) = &cell.source {
                if matches(&src.display) {
                    let sig = if src.is_procedure {
                        format!("Call {}({} : {})", src.display, src.params.join(", "), src.outputs.join(", "))
                    } else {
                        format!("{}({})", src.display, src.params.join(", "))
                    };
                    out.push(Candidate { label: src.display.clone(), insert: format!("{}(", src.display), kind: Kind::UserFunction, detail: sig });
                }
            }
        }
        let mut seen = std::collections::HashSet::new();
        for (i, v) in a.model.vars.iter().enumerate() {
            let (label, insert) = if v.index.is_empty() {
                (v.display.clone(), v.display.clone())
            } else {
                let base = v.display.split('[').next().unwrap_or("").to_string();
                (format!("{base}[]"), format!("{base}["))
            };
            if !matches(&label) || !seen.insert(label.to_lowercase()) || label.eq_ignore_ascii_case(prefix) {
                continue;
            }
            let st = a.plan.structure.var_status[i];
            let mut detail = format!("{st:?}");
            if v.index.is_empty() {
                if let Some(sol) = &a.solution {
                    if sol.known[i] {
                        detail = format!("= {}", fmt_num(sol.x[i]));
                    }
                }
            }
            out.push(Candidate { label, insert, kind: Kind::Variable, detail });
        }
    }
    out.sort_by_key(|c| (!c.label.to_lowercase().starts_with(&pl), c.kind == Kind::Keyword, c.label.len()));
    out
}

/// Signature of the innermost call enclosing the cursor.
pub fn signature_at(text: &str, cur: usize, ectx: &EditorContext) -> Option<String> {
    let chars: Vec<char> = text.chars().take(cur).collect();
    let mut depth = 0i32;
    let mut i = chars.len();
    while i > 0 {
        i -= 1;
        match chars[i] {
            '\n' if depth >= 0 => return None,
            ')' => depth += 1,
            '(' => {
                if depth == 0 {
                    let mut j = i;
                    while j > 0 && is_ident_char(chars[j - 1]) {
                        j -= 1;
                    }
                    let name: String = chars[j..i].iter().collect::<String>().to_lowercase();
                    if let Some((_, d)) = dfcore::props::PROPERTY_FUNCTIONS.iter().find(|(n, _)| *n == name) {
                        return Some(d.to_string());
                    }
                    if let Some((_, d)) = dfcore::compile::MATH_FUNCTIONS.iter().find(|(n, _)| *n == name) {
                        return Some(d.to_string());
                    }
                    if let Some(a) = ectx.analysis {
                        for cell in &a.model.funcs {
                            if let Some(s) = &cell.source {
                                if s.name == name {
                                    return Some(if s.is_procedure {
                                        format!("{}({} : {})", s.display, s.params.join(", "), s.outputs.join(", "))
                                    } else {
                                        format!("{}({})", s.display, s.params.join(", "))
                                    });
                                }
                            }
                        }
                    }
                    return None;
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

pub fn fmt_num(v: f64) -> String {
    if !v.is_finite() {
        return format!("{v}");
    }
    let a = v.abs();
    if a != 0.0 && (a >= 1e7 || a < 1e-4) {
        format!("{v:.6e}")
    } else {
        let s = format!("{v:.6}");
        let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
        if s.is_empty() || s == "-" {
            "0".into()
        } else {
            s
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brackets_pair_in_code_and_comments() {
        // code ( [ ] ), a brace comment with a nested pair and a '(' inside,
        // a quoted comment with a '(' inside, and an unmatched ')'
        let t = "f(a[1]) {x (y {z})} \"(\" )";
        let p = bracket_pairs(t);
        let at = |c: char, n: usize| t.char_indices().filter(|x| x.1 == c).nth(n).map(|x| t[..x.0].chars().count()).unwrap();
        assert_eq!(p[&at('(', 0)], Some(at(')', 0)));
        assert_eq!(p[&at('[', 0)], Some(at(']', 0)));
        assert_eq!(p[&at('{', 0)], Some(at('}', 1)));
        assert_eq!(p[&at('{', 1)], Some(at('}', 0)));
        assert!(!p.contains_key(&at('(', 1)), "brackets in brace comments are text");
        assert!(!p.contains_key(&at('(', 2)), "brackets in quoted comments are text");
        assert_eq!(p[&at(')', 2)], None, "unmatched");
    }
}
