//! Convenience helper for producing coloured terminal output.
//!
//! This optional helper applies terminal colours (or other effects which
//! can be achieved using inline characters sent to the terminal such as
//! underlining in some terminals).

use uuid::Uuid;

use crate::{parse, RichAnnotation, RichDecorator, RenderTree};
use std::{io, vec};

#[allow(missing_docs)]
#[derive(Debug, Clone)]
pub enum Control {
    Default,
    RedactedBegin(String, uuid::Uuid),
    RedactedEnd(Uuid),
    Str(String),
    NoBreakBegin,
    NoBreakEnd,
    /// src, width, height
    Image(String, usize, usize),
    Bell(String),
    LF,
    StrRedacted(String,uuid::Uuid),
    Audio(String)
}
/// 仅解析,与高度宽度无关
/// 密码区段的UUid在此过程生成，为了不必重新输入密码，应将此渲染树存储
pub fn just_parse<R>(input:R) -> RenderTree
where R: io::Read
{
    parse(input)
}
/// 仅渲染
pub fn just_render<FMap>(
    input: RenderTree,
    width: usize,
    map: FMap,
) -> Result<Vec<Control>, std::fmt::Error>
where
    FMap: Fn(&RichAnnotation) -> (String, Box<dyn Fn(&String) -> String>, String),
{
    let lines =input
        .render(width, RichDecorator::new())
        .into_lines();
    let mut cmds: Vec<Control> = vec![];
    html_trace!("循环开始: lines:{:#?}", lines);
    let mut redacted_stack:Vec<Uuid> = vec![];
    for line in lines {
        let mut is_marker = false;
        for ts in line.tagged_strings() {
            let mut start = String::new();
            let mut finish = String::new();
            let mut content = String::new();
            let mut mutated = false;
            is_marker = false;
            for ann in &ts.tag {
                match ann {
                    RichAnnotation::NoBreakBegin => {
                        assert!(&ts.s.is_empty());
                        is_marker = true;
                        cmds.push(Control::NoBreakBegin);
                    }
                    RichAnnotation::RedactedBegin(psk, id) => {
                        assert!(&ts.s.is_empty());
                        is_marker = true;
                        redacted_stack.push(*id);
                        // cmds.push(Control::RedactedBegin(psk.to_string(), *id));
                    }
                    RichAnnotation::Image(src, w, h) => {
                        if w * h >= 1 {
                            // assert!(&ts.s.is_empty());
                            is_marker = true;
                            cmds.push(Control::Image(src.to_string(), *w, *h))
                        } else {
                        }
                    },
                    RichAnnotation::RedactedEnd(_, id) => {
                        assert!(&ts.s.is_empty());
                        is_marker = true;
                        // cmds.push(Control::RedactedEnd(*id));
                        assert!(redacted_stack.last().unwrap()==id,"密码区段不得嵌套");
                        redacted_stack.pop();
                    },
                    RichAnnotation::NoBreakEnd => {
                        assert!(&ts.s.is_empty());
                        is_marker = true;
                        cmds.push(Control::NoBreakEnd)},
                    RichAnnotation::Custom(typ, value) => {
                        if typ == "audio" {
                            assert!(!value.is_empty());
                            is_marker = true;
                            cmds.push(Control::Audio(value[0].clone()))
                        } else {
                            html_trace!("遇到不认识的Custom 注解");
                        }
                    }
                    _ => (),
                }
            }
            if is_marker {
                break;
            }

            for ann in &ts.tag {
                mutated = true;
                let (s, mutator, f) = map(ann);
                start.push_str(&s);
                finish.push_str(&f);
                html_trace!("变化前:{:?}", &ts.s);
                html_trace!("变化后:{:?}", mutator(&ts.s));
                content.push_str(&mutator(&ts.s));
            }
            let mut s = String::new();
            if mutated {
                s += format!("{}{}{}", start, content, finish).as_str();
            } else {
                s += format!("{}{}{}", start, ts.s, finish).as_str();
            }
            if let Some(id) = redacted_stack.last() {
                cmds.push(Control::StrRedacted(s, *id))
            } else {
                cmds.push(Control::Str(s))
            }
        }
        if !is_marker {
            cmds.push(Control::LF);
        }
        // html_trace!("YLY: 单元高度:{},单元内容：{:?}",&unit.lines().count(),&unit);
    }

    html_trace!("segments:{:?}", cmds);
    Ok(cmds)
}

/// 重要
pub fn custom_render<R, FMap>(
    input: R,
    width: usize,
    map: FMap,
) -> Result<Vec<Control>, std::fmt::Error>
where
    R: io::Read,
    FMap: Fn(&RichAnnotation) -> (String, Box<dyn Fn(&String) -> String>, String),
{
    let lines = parse(input)
        .render(width, RichDecorator::new())
        .into_lines();
    let mut cmds: Vec<Control> = vec![];
    html_trace!("循环开始: lines:{:#?}", lines);
    let mut redacted_stack:Vec<Uuid> = vec![];
    for line in lines {
        let mut is_marker = false;
        for ts in line.tagged_strings() {
            let mut start = String::new();
            let mut finish = String::new();
            let mut content = String::new();
            let mut mutated = false;
            is_marker = false;
            for ann in &ts.tag {
                match ann {
                    RichAnnotation::NoBreakBegin => {
                        assert!(&ts.s.is_empty());
                        is_marker = true;
                        cmds.push(Control::NoBreakBegin);
                    }
                    RichAnnotation::RedactedBegin(psk, id) => {
                        assert!(&ts.s.is_empty());
                        is_marker = true;
                        redacted_stack.push(*id);
                        // cmds.push(Control::RedactedBegin(psk.to_string(), *id));
                    }
                    RichAnnotation::Image(src, w, h) => {
                        if w * h >= 1 {
                            // assert!(&ts.s.is_empty());
                            is_marker = true;
                            cmds.push(Control::Image(src.to_string(), *w, *h))
                        } else {
                        }
                    },
                    RichAnnotation::RedactedEnd(_, id) => {
                        assert!(&ts.s.is_empty());
                        is_marker = true;
                        // cmds.push(Control::RedactedEnd(*id));
                        assert!(redacted_stack.last().unwrap()==id,"密码区段不得嵌套");
                        redacted_stack.pop();
                    },
                    RichAnnotation::NoBreakEnd => {
                        assert!(&ts.s.is_empty());
                        is_marker = true;
                        cmds.push(Control::NoBreakEnd)},
                    RichAnnotation::Custom(typ, value) => {
                        if typ == "audio" {
                            assert!(!value.is_empty());
                            is_marker = true;
                            cmds.push(Control::Audio(value[0].clone()))
                        } else {
                            html_trace!("遇到不认识的Custom 注解");
                        }
                    }
                    _ => (),
                }
            }
            if is_marker {
                break;
            }

            for ann in &ts.tag {
                mutated = true;
                let (s, mutator, f) = map(ann);
                start.push_str(&s);
                finish.push_str(&f);
                html_trace!("变化前:{:?}", &ts.s);
                html_trace!("变化后:{:?}", mutator(&ts.s));
                content.push_str(&mutator(&ts.s));
            }
            let mut s = String::new();
            if mutated {
                s += format!("{}{}{}", start, content, finish).as_str();
            } else {
                s += format!("{}{}{}", start, ts.s, finish).as_str();
            }
            if let Some(id) = redacted_stack.last() {
                cmds.push(Control::StrRedacted(s, *id))
            } else {
                cmds.push(Control::Str(s))
            }
        }
        if !is_marker {
            cmds.push(Control::LF);
        }
        // html_trace!("YLY: 单元高度:{},单元内容：{:?}",&unit.lines().count(),&unit);
    }

    html_trace!("segments:{:?}", cmds);
    Ok(cmds)
}

/// 排版用盒子
#[derive(Debug,Clone)]
pub struct PageBlock {
    /// 盒子里的控制序列
    pub inner: Vec<Control>,
    /// 盒子的高度
    pub height: usize
}
impl Default for PageBlock{
    fn default() -> Self {
        PageBlock { inner: vec![], height: 0 }
    }
}
/// 生成盒子供排版用
pub fn try_build_block(controls:&Vec<Control>)->Vec<PageBlock>{
    let mut blocks = vec![];
    let mut block = PageBlock { inner: vec![], height: 0 };
    let mut no_break :bool =false;
    for c in controls {
        match c {
            Control::Default => unreachable!(),
            Control::RedactedBegin(_, _) => unreachable!(),
            Control::RedactedEnd(_) => unreachable!(),
            Control::LF => {
                block.inner.push(Control::LF);
                block.height += 1;
                if !no_break {
                    blocks.push(block);
                    block = PageBlock::default();

                }
            },
            Control::NoBreakBegin => {
                if no_break {
                    panic!("Section禁止嵌套");
                };
                no_break = true;
                if !block.inner.is_empty() {
                    blocks.push(block);
                    block = PageBlock::default();
                }
            },
            Control::NoBreakEnd => {
                if !no_break{
                    panic!("Section不匹配");
                }
                no_break = false;
                blocks.push(block);
                block = PageBlock::default();
            },
            Control::Image(src, w, h) => {
                if !block.inner.is_empty() {
                    blocks.push(block);
                    block = PageBlock::default();
                }
                block.inner.push(Control::Image(src.to_string(), *w, *h));
                block.height += *h;
                blocks.push(block);
                block=PageBlock::default();
            },
            // Control::Str(_)
            // |Control::StrRedacted(_, _)
            // |Control::Bell(_)
            // |Control::Audio(_)
            x => block.inner.push(x.clone()),
        }
    }
    blocks
}