//! Terminal-independent document model: blocks and inline spans (SPEC §3.3, §3.11).

use std::io;
use std::path::Path;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

/// A parsed file: a block tree plus the flags the viewer needs (SPEC §4.3, §4.4).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Document {
    pub blocks: Vec<Block>,
    /// The input was not valid UTF-8 and was decoded lossily.
    pub invalid_utf8: bool,
    /// The input holds no content at all.
    pub empty: bool,
}

/// A block-level element. More variants (tables, footnotes, images) can be added later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Paragraph(Vec<Inline>),
    Heading {
        level: u8,
        content: Vec<Inline>,
    },
    List(List),
    BlockQuote(Vec<Block>),
    CodeBlock {
        lang: Option<String>,
        text: String,
    },
    Rule,
    Html(String),
    /// Raw Mermaid source of a diagram.
    Diagram(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct List {
    /// `Some(start)` for an ordered list, `None` for a bullet list.
    pub start: Option<u64>,
    pub items: Vec<ListItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    /// `Some(checked)` for a task list item.
    pub task: Option<bool>,
    pub blocks: Vec<Block>,
}

/// An inline element of a text block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    Span(Span),
    Html(String),
    SoftBreak,
    HardBreak,
}

/// A run of text with uniform style and an optional link target.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Span {
    pub text: String,
    pub style: Style,
    pub link: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Style {
    pub emphasis: bool,
    pub strong: bool,
    pub strikethrough: bool,
    pub code: bool,
}

/// Kind of source being parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Markdown,
    Mermaid,
}

impl Source {
    /// Pick the source kind from the file extension.
    pub fn from_path(path: &Path) -> Source {
        match path.extension().and_then(|e| e.to_str()) {
            Some(e) if e.eq_ignore_ascii_case("mmd") || e.eq_ignore_ascii_case("mermaid") => {
                Source::Mermaid
            }
            _ => Source::Markdown,
        }
    }
}

impl Document {
    /// Decode `bytes` lossily and parse them as `source`.
    pub fn from_bytes(bytes: &[u8], source: Source) -> Document {
        let invalid_utf8 = std::str::from_utf8(bytes).is_err();
        let text = String::from_utf8_lossy(bytes);
        let empty = text.is_empty();
        let blocks = match source {
            _ if empty => Vec::new(),
            Source::Markdown => parse_markdown(&text),
            Source::Mermaid => vec![Block::Diagram(text.into_owned())],
        };
        Document {
            blocks,
            invalid_utf8,
            empty,
        }
    }

    pub fn from_path(path: &Path) -> io::Result<Document> {
        let bytes = std::fs::read(path)?;
        Ok(Document::from_bytes(&bytes, Source::from_path(path)))
    }
}

fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
}

/// A block under construction.
enum Frame {
    Root(Vec<Block>),
    Paragraph(Vec<Inline>),
    Heading(u8, Vec<Inline>),
    List(Option<u64>, Vec<ListItem>),
    Item(Option<bool>, Vec<Block>),
    Quote(Vec<Block>),
    Code(Option<String>, String),
    Html(String),
    /// Container whose events are not modelled yet; its content is dropped.
    Skipped,
}

struct Builder {
    stack: Vec<Frame>,
    style: Style,
    link: Vec<String>,
}

fn parse_markdown(text: &str) -> Vec<Block> {
    let mut b = Builder {
        stack: vec![Frame::Root(Vec::new())],
        style: Style::default(),
        link: Vec::new(),
    };
    for event in Parser::new_ext(text, options()) {
        b.event(event);
    }
    match b.stack.pop() {
        Some(Frame::Root(blocks)) => blocks,
        _ => Vec::new(),
    }
}

impl Builder {
    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(end) => self.end(end),
            Event::Text(t) => match self.stack.last_mut() {
                Some(Frame::Code(_, buf) | Frame::Html(buf)) => buf.push_str(&t),
                _ => self.push_span(&t, Style::default()),
            },
            Event::Code(t) => self.push_span(
                &t,
                Style {
                    code: true,
                    ..Style::default()
                },
            ),
            Event::Html(t) => match self.stack.last_mut() {
                Some(Frame::Html(buf)) => buf.push_str(&t),
                _ => self.push_inline(Inline::Html(t.into_string())),
            },
            Event::InlineHtml(t) => self.push_inline(Inline::Html(t.into_string())),
            Event::SoftBreak => self.push_inline(Inline::SoftBreak),
            Event::HardBreak => self.push_inline(Inline::HardBreak),
            Event::Rule => self.push_block(Block::Rule),
            Event::TaskListMarker(checked) => {
                if let Some(Frame::Item(task, _)) = self.stack.last_mut() {
                    *task = Some(checked);
                }
            }
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        let frame = match tag {
            Tag::Paragraph => Frame::Paragraph(Vec::new()),
            Tag::Heading { level, .. } => Frame::Heading(level as u8, Vec::new()),
            Tag::BlockQuote(_) => Frame::Quote(Vec::new()),
            Tag::CodeBlock(kind) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(l) => l.split_whitespace().next().map(str::to_owned),
                    CodeBlockKind::Indented => None,
                };
                Frame::Code(lang, String::new())
            }
            Tag::HtmlBlock => Frame::Html(String::new()),
            Tag::List(start) => Frame::List(start, Vec::new()),
            Tag::Item => Frame::Item(None, Vec::new()),
            Tag::Emphasis => return self.style.emphasis = true,
            Tag::Strong => return self.style.strong = true,
            Tag::Strikethrough => return self.style.strikethrough = true,
            Tag::Link { dest_url, .. } => return self.link.push(dest_url.into_string()),
            // Tables, footnotes, images, ...: not modelled yet.
            _ => Frame::Skipped,
        };
        self.stack.push(frame);
    }

    fn end(&mut self, end: TagEnd) {
        match end {
            TagEnd::Emphasis => return self.style.emphasis = false,
            TagEnd::Strong => return self.style.strong = false,
            TagEnd::Strikethrough => return self.style.strikethrough = false,
            TagEnd::Link => return drop(self.link.pop()),
            _ => {}
        }
        let Some(frame) = self.stack.pop() else {
            return;
        };
        match frame {
            Frame::Paragraph(c) => self.push_block(Block::Paragraph(c)),
            Frame::Heading(level, content) => self.push_block(Block::Heading { level, content }),
            Frame::Quote(blocks) => self.push_block(Block::BlockQuote(blocks)),
            Frame::Code(lang, text) => self.push_block(Block::CodeBlock { lang, text }),
            Frame::Html(t) => self.push_block(Block::Html(t)),
            Frame::List(start, items) => self.push_block(Block::List(List { start, items })),
            Frame::Item(task, blocks) => {
                if let Some(Frame::List(_, items)) = self.stack.last_mut() {
                    items.push(ListItem { task, blocks });
                }
            }
            Frame::Skipped | Frame::Root(_) => {}
        }
    }

    fn push_span(&mut self, text: &str, extra: Style) {
        let style = Style {
            emphasis: self.style.emphasis | extra.emphasis,
            strong: self.style.strong | extra.strong,
            strikethrough: self.style.strikethrough | extra.strikethrough,
            code: self.style.code | extra.code,
        };
        let link = self.link.last().cloned();
        self.push_inline(Inline::Span(Span {
            text: text.to_owned(),
            style,
            link,
        }));
    }

    /// Append to the open text block; tight list items have none, so open a paragraph.
    fn push_inline(&mut self, inline: Inline) {
        match self.stack.last_mut() {
            Some(Frame::Paragraph(c) | Frame::Heading(_, c)) => c.push(inline),
            Some(Frame::Item(_, blocks) | Frame::Quote(blocks)) => {
                if let Some(Block::Paragraph(c)) = blocks.last_mut() {
                    c.push(inline);
                } else {
                    blocks.push(Block::Paragraph(vec![inline]));
                }
            }
            _ => {}
        }
    }

    fn push_block(&mut self, block: Block) {
        if let Some(Frame::Root(b) | Frame::Item(_, b) | Frame::Quote(b)) = self.stack.last_mut() {
            b.push(block);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn md(s: &str) -> Document {
        Document::from_bytes(s.as_bytes(), Source::Markdown)
    }

    fn plain(text: &str) -> Inline {
        Inline::Span(Span {
            text: text.into(),
            ..Span::default()
        })
    }

    fn styled(text: &str, style: Style) -> Inline {
        Inline::Span(Span {
            text: text.into(),
            style,
            link: None,
        })
    }

    #[test]
    fn heading_levels_1_to_6() {
        let doc = md("# a\n## b\n### c\n#### d\n##### e\n###### f\n");
        assert_eq!(doc.blocks.len(), 6);
        for (i, b) in doc.blocks.iter().enumerate() {
            let Block::Heading { level, content } = b else {
                panic!("not heading: {b:?}")
            };
            assert_eq!(*level as usize, i + 1);
            assert_eq!(content.len(), 1);
        }
        assert_eq!(
            doc.blocks[1],
            Block::Heading {
                level: 2,
                content: vec![plain("b")]
            }
        );
    }

    #[test]
    fn inline_styles() {
        let doc = md("*e* **s** ~~x~~ `c` ***both***");
        let Block::Paragraph(p) = &doc.blocks[0] else {
            panic!()
        };
        let e = Style {
            emphasis: true,
            ..Style::default()
        };
        let s = Style {
            strong: true,
            ..Style::default()
        };
        let x = Style {
            strikethrough: true,
            ..Style::default()
        };
        let c = Style {
            code: true,
            ..Style::default()
        };
        let both = Style {
            emphasis: true,
            strong: true,
            ..Style::default()
        };
        assert_eq!(
            p,
            &vec![
                styled("e", e),
                plain(" "),
                styled("s", s),
                plain(" "),
                styled("x", x),
                plain(" "),
                styled("c", c),
                plain(" "),
                styled("both", both),
            ]
        );
    }

    #[test]
    fn nested_lists_keep_depth_and_start() {
        let doc = md("3. one\n   - a\n   - b\n4. two\n");
        let Block::List(l) = &doc.blocks[0] else {
            panic!()
        };
        assert_eq!(l.start, Some(3));
        assert_eq!(l.items.len(), 2);
        let Block::List(inner) = &l.items[0].blocks[1] else {
            panic!("{:?}", l.items[0])
        };
        assert_eq!(inner.start, None);
        assert_eq!(inner.items.len(), 2);
        assert_eq!(l.items[0].blocks[0], Block::Paragraph(vec![plain("one")]));
    }

    #[test]
    fn task_items_carry_state() {
        let doc = md("- [x] done\n- [ ] todo\n- plain\n");
        let Block::List(l) = &doc.blocks[0] else {
            panic!()
        };
        let tasks: Vec<_> = l.items.iter().map(|i| i.task).collect();
        assert_eq!(tasks, vec![Some(true), Some(false), None]);
    }

    #[test]
    fn nested_block_quotes() {
        let doc = md("> a\n>\n> > b\n");
        let Block::BlockQuote(outer) = &doc.blocks[0] else {
            panic!()
        };
        assert_eq!(outer[0], Block::Paragraph(vec![plain("a")]));
        let Block::BlockQuote(inner) = &outer[1] else {
            panic!()
        };
        assert_eq!(inner[0], Block::Paragraph(vec![plain("b")]));
    }

    #[test]
    fn fenced_code_keeps_lang_and_content() {
        let doc = md("```rust\nfn main() {}\n\n  x\n```\n\n```\nno lang\n```\n");
        assert_eq!(
            doc.blocks[0],
            Block::CodeBlock {
                lang: Some("rust".into()),
                text: "fn main() {}\n\n  x\n".into()
            }
        );
        assert_eq!(
            doc.blocks[1],
            Block::CodeBlock {
                lang: None,
                text: "no lang\n".into()
            }
        );
    }

    #[test]
    fn html_blocks_and_inline_html_are_raw() {
        let doc = md("<div>\nhi\n</div>\n\nsome <b>bold</b> text\n");
        assert_eq!(doc.blocks[0], Block::Html("<div>\nhi\n</div>\n".into()));
        let Block::Paragraph(p) = &doc.blocks[1] else {
            panic!()
        };
        assert!(p.contains(&Inline::Html("<b>".into())));
        assert!(p.contains(&Inline::Html("</b>".into())));
    }

    #[test]
    fn thematic_break_is_rule() {
        let doc = md("a\n\n---\n\nb\n");
        assert_eq!(doc.blocks[1], Block::Rule);
        assert_eq!(doc.blocks.len(), 3);
    }

    #[test]
    fn links_carry_target_and_text() {
        let doc = md("see [the **docs**](https://x.io/a) now\n");
        let Block::Paragraph(p) = &doc.blocks[0] else {
            panic!()
        };
        let linked: Vec<_> = p
            .iter()
            .filter_map(|i| match i {
                Inline::Span(s) if s.link.is_some() => Some(s),
                _ => None,
            })
            .collect();
        assert_eq!(linked.len(), 2);
        assert!(
            linked
                .iter()
                .all(|s| s.link.as_deref() == Some("https://x.io/a"))
        );
        assert_eq!(linked[0].text, "the ");
        assert_eq!(linked[1].text, "docs");
        assert!(linked[1].style.strong);
    }

    #[test]
    fn breaks_soft_and_hard() {
        let doc = md("a\nb  \nc\n");
        let Block::Paragraph(p) = &doc.blocks[0] else {
            panic!()
        };
        assert_eq!(
            p,
            &vec![
                plain("a"),
                Inline::SoftBreak,
                plain("b"),
                Inline::HardBreak,
                plain("c")
            ]
        );
    }

    #[test]
    fn invalid_utf8_is_lossy_and_flagged() {
        let doc = Document::from_bytes(b"ok \xff\xfe end", Source::Markdown);
        assert!(doc.invalid_utf8);
        let Block::Paragraph(p) = &doc.blocks[0] else {
            panic!()
        };
        let Inline::Span(s) = &p[0] else { panic!() };
        assert!(s.text.contains('\u{FFFD}'));
        assert!(!md("fine").invalid_utf8);
    }

    #[test]
    fn empty_file_is_empty_document() {
        let doc = md("");
        assert!(doc.empty);
        assert!(doc.blocks.is_empty());
        assert!(!md("x").empty);
    }

    #[test]
    fn mmd_file_is_single_diagram_block() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["a.mmd", "b.mermaid"] {
            let p = dir.path().join(name);
            std::fs::write(&p, "graph TD\n  A-->B\n").unwrap();
            let doc = Document::from_path(&p).unwrap();
            assert_eq!(
                doc.blocks,
                vec![Block::Diagram("graph TD\n  A-->B\n".into())]
            );
        }
    }

    #[test]
    fn content_after_image_is_kept() {
        let doc = md("text ![alt](x.png) more\n\n# Heading\n");
        assert_eq!(doc.blocks.len(), 2);
        let Block::Paragraph(p) = &doc.blocks[0] else {
            panic!()
        };
        assert_eq!(p.first(), Some(&plain("text ")));
        assert_eq!(p.last(), Some(&plain(" more")));
        assert!(matches!(doc.blocks[1], Block::Heading { level: 1, .. }));
    }

    #[test]
    fn tight_item_inlines_form_one_paragraph() {
        let doc = md("- a **b** c\n  - nested\n");
        let Block::List(l) = &doc.blocks[0] else {
            panic!()
        };
        let blocks = &l.items[0].blocks;
        assert_eq!(blocks.len(), 2);
        let Block::Paragraph(p) = &blocks[0] else {
            panic!()
        };
        assert_eq!(p.len(), 3);
        assert!(matches!(blocks[1], Block::List(_)));
    }
}
