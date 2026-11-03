//! Minimal CommonMark-style renderer for the built-in help: headings,
//! paragraphs, bullet and numbered lists, fenced code, tables, block quotes,
//! rules, and inline `code`, **bold**, *italic* and [links](url).

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId, RichText, TextStyle, Ui};

enum Block {
    Heading(usize, String),
    Para(String),
    Item { indent: usize, marker: String, text: String },
    Code(String),
    Table(Vec<Vec<String>>),
    Quote(String),
    Rule,
}

fn list_marker(l: &str) -> Option<(usize, String, String)> {
    let indent = l.len() - l.trim_start().len();
    let t = l.trim_start();
    for b in ["* ", "- ", "+ "] {
        if let Some(r) = t.strip_prefix(b) {
            return Some((indent, "•".into(), r.to_string()));
        }
    }
    let d: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !d.is_empty() {
        if let Some(r) = t[d.len()..].strip_prefix(". ").or_else(|| t[d.len()..].strip_prefix(") ")) {
            return Some((indent, format!("{d}."), r.to_string()));
        }
    }
    None
}

fn cells(l: &str) -> Vec<String> {
    let t = l.trim().trim_start_matches('|').trim_end_matches('|');
    t.split('|').map(|c| c.trim().to_string()).collect()
}

fn parse(src: &str) -> Vec<Block> {
    let lines: Vec<&str> = src.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let l = lines[i];
        let t = l.trim();
        if t.is_empty() {
            i += 1;
        } else if t.starts_with("```") {
            let mut code = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                code.push(lines[i]);
                i += 1;
            }
            i += 1;
            out.push(Block::Code(code.join("\n")));
        } else if t.starts_with('#') {
            let n = t.chars().take_while(|c| *c == '#').count();
            out.push(Block::Heading(n, t[n..].trim().to_string()));
            i += 1;
        } else if t.chars().all(|c| c == '-' || c == '*' || c == '_') && t.len() >= 3 {
            out.push(Block::Rule);
            i += 1;
        } else if t.starts_with('|') {
            let mut rows = Vec::new();
            while i < lines.len() && lines[i].trim().starts_with('|') {
                let r = cells(lines[i]);
                // skip the |---|:--:| separator row
                if !r.iter().all(|c| !c.is_empty() && c.chars().all(|ch| matches!(ch, '-' | ':' | ' '))) {
                    rows.push(r);
                }
                i += 1;
            }
            out.push(Block::Table(rows));
        } else if t.starts_with('>') {
            let mut q = Vec::new();
            while i < lines.len() && lines[i].trim_start().starts_with('>') {
                q.push(lines[i].trim_start()[1..].trim().to_string());
                i += 1;
            }
            out.push(Block::Quote(q.join(" ")));
        } else if let Some((indent, marker, first)) = list_marker(l) {
            let mut text = vec![first];
            i += 1;
            // continuation lines: indented, not a new item or block
            while i < lines.len() {
                let n = lines[i];
                if n.trim().is_empty() || list_marker(n).is_some() || !n.starts_with(' ') || n.trim_start().starts_with("```") {
                    break;
                }
                text.push(n.trim().to_string());
                i += 1;
            }
            out.push(Block::Item { indent, marker, text: text.join(" ") });
        } else {
            let mut p = Vec::new();
            while i < lines.len() {
                let n = lines[i].trim();
                if n.is_empty() || n.starts_with('#') || n.starts_with("```") || n.starts_with('|') || n.starts_with('>') || list_marker(lines[i]).is_some() {
                    break;
                }
                p.push(n);
                i += 1;
            }
            out.push(Block::Para(p.join(" ")));
        }
    }
    out
}

/// Inline markup to a layout job.
fn inline(ui: &Ui, s: &str, size: f32, strong: bool) -> LayoutJob {
    let v = ui.visuals();
    let base = if strong { v.strong_text_color() } else { v.text_color() };
    let prop = FontId::proportional(size);
    let mono = FontId::monospace(size * 0.95);
    let mut job = LayoutJob::default();
    job.wrap.max_width = ui.available_width();
    let fmt = |bold: bool, ital: bool| TextFormat { font_id: prop.clone(), color: if bold { v.strong_text_color() } else { base }, italics: ital, ..Default::default() };
    let (mut bold, mut ital) = (false, false);
    let mut buf = String::new();
    let flush = |job: &mut LayoutJob, buf: &mut String, f: TextFormat| {
        if !buf.is_empty() {
            job.append(buf, 0.0, f);
            buf.clear();
        }
    };
    let c: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if ch == '\\' && i + 1 < c.len() {
            buf.push(c[i + 1]);
            i += 2;
        } else if ch == '`' {
            if let Some(j) = c[i + 1..].iter().position(|x| *x == '`') {
                flush(&mut job, &mut buf, fmt(bold, ital));
                let code: String = c[i + 1..i + 1 + j].iter().collect();
                job.append(&code, 0.0, TextFormat { font_id: mono.clone(), color: base, background: v.code_bg_color, ..Default::default() });
                i += j + 2;
            } else {
                buf.push(ch);
                i += 1;
            }
        } else if (ch == '*' || ch == '_') && i + 1 < c.len() && c[i + 1] == ch {
            flush(&mut job, &mut buf, fmt(bold, ital));
            bold = !bold;
            i += 2;
        } else if ch == '*' || (ch == '_' && (i == 0 || !c[i - 1].is_alphanumeric()) != (i + 1 >= c.len() || !c[i + 1].is_alphanumeric())) {
            // '*' always; '_' only at a word boundary (snake_case stays)
            flush(&mut job, &mut buf, fmt(bold, ital));
            ital = !ital;
            i += 1;
        } else if ch == '[' {
            let close = c[i..].iter().position(|x| *x == ']').map(|k| i + k);
            match close {
                Some(e) if e + 1 < c.len() && c[e + 1] == '(' => {
                    if let Some(k) = c[e + 1..].iter().position(|x| *x == ')') {
                        flush(&mut job, &mut buf, fmt(bold, ital));
                        let label: String = c[i + 1..e].iter().collect();
                        job.append(
                            &label,
                            0.0,
                            TextFormat { font_id: prop.clone(), color: v.hyperlink_color, underline: egui::Stroke::new(1.0, v.hyperlink_color), ..Default::default() },
                        );
                        i = e + 1 + k + 1;
                    } else {
                        buf.push(ch);
                        i += 1;
                    }
                }
                _ => {
                    buf.push(ch);
                    i += 1;
                }
            }
        } else {
            buf.push(ch);
            i += 1;
        }
    }
    flush(&mut job, &mut buf, fmt(bold, ital));
    job
}

/// Render markdown `src` into `ui`.
pub fn show(ui: &mut Ui, src: &str) {
    let body = ui.style().text_styles.get(&TextStyle::Body).map(|f| f.size).unwrap_or(14.0);
    for (k, b) in parse(src).iter().enumerate() {
        match b {
            Block::Heading(n, t) => {
                if k > 0 {
                    ui.add_space(if *n <= 2 { 14.0 } else { 8.0 });
                }
                let size = match n {
                    1 => body * 1.6,
                    2 => body * 1.3,
                    3 => body * 1.12,
                    _ => body,
                };
                ui.label(inline(ui, t, size, true));
                if *n <= 2 {
                    ui.separator();
                }
                ui.add_space(2.0);
            }
            Block::Para(t) => {
                ui.label(inline(ui, t, body, false));
                ui.add_space(6.0);
            }
            Block::Item { indent, marker, text } => {
                ui.horizontal_top(|ui| {
                    ui.add_space(8.0 + 12.0 * (*indent as f32 / 2.0));
                    ui.label(RichText::new(marker).strong());
                    ui.label(inline(ui, text, body, false));
                });
            }
            Block::Code(t) => {
                ui.add_space(2.0);
                egui::Frame::new().fill(ui.visuals().code_bg_color).corner_radius(4.0).inner_margin(8.0).show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    egui::ScrollArea::horizontal().id_salt(("md_code", k)).show(ui, |ui| {
                        ui.add(egui::Label::new(RichText::new(t).monospace()).extend());
                    });
                });
                ui.add_space(6.0);
            }
            Block::Table(rows) => {
                egui::Grid::new(("md_table", k)).striped(true).spacing([16.0, 4.0]).show(ui, |ui| {
                    for (r, row) in rows.iter().enumerate() {
                        for c in row {
                            ui.label(inline(ui, c, body, r == 0));
                        }
                        ui.end_row();
                    }
                });
                ui.add_space(6.0);
            }
            Block::Quote(t) => {
                ui.horizontal_top(|ui| {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(3.0, body * 1.4), egui::Sense::hover());
                    ui.painter().rect_filled(r, 0.0, Color32::from_gray(140));
                    ui.label(inline(ui, t, body, false));
                });
                ui.add_space(6.0);
            }
            Block::Rule => {
                ui.separator();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_blocks() {
        let b = parse("# T\n\npara\nline\n\n* a\n  cont\n1. b\n\n```\ncode\n```\n| h | i |\n|---|---|\n| 1 | 2 |\n");
        assert!(matches!(&b[0], Block::Heading(1, t) if t == "T"));
        assert!(matches!(&b[1], Block::Para(t) if t == "para line"));
        assert!(matches!(&b[2], Block::Item { text, .. } if text == "a cont"));
        assert!(matches!(&b[3], Block::Item { marker, .. } if marker == "1."));
        assert!(matches!(&b[4], Block::Code(t) if t == "code"));
        assert!(matches!(&b[5], Block::Table(r) if r.len() == 2 && r[1] == vec!["1".to_string(), "2".to_string()]));
    }
}
