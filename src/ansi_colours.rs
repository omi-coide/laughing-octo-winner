//! Convenience helper for producing coloured terminal output.
//!
//! This optional helper applies terminal colours (or other effects which
//! can be achieved using inline characters sent to the terminal such as
//! underlining in some terminals).

use uuid::Uuid;

use crate::{parse, RichAnnotation, RichDecorator};
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
    Image(String,usize,usize)
}
/* pub struct Page {
    pub width: usize,
    pub height: usize,
    pub lines: Vec<(String, Control)>,
}
pub struct Article {
    pages: Vec<Page>,
    passwords: HashMap<Uuid, (String, bool)>,
} */
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
    for line in lines {
        for ts in line.tagged_strings() {
            let mut start = String::new();
            let mut finish = String::new();
            let mut content = String::new();
            let mut mutated = false;
            let mut is_img = false;
            for ann in &ts.tag {
                match ann {
                    RichAnnotation::NoBreakBegin => cmds.push(Control::NoBreakBegin),
                    RichAnnotation::RedactedBegin(psk, id) => {
                        cmds.push(Control::RedactedBegin(psk.to_string(), *id))
                    },
                    RichAnnotation::Image(src, w, h) => {
                        if w*h >=1 {
                            is_img = true;
                            cmds.push(Control::Image(src.to_string(), *w, *h))
                        } else {

                        }
                    }
                    _ => (),
                }
            };
            if is_img {
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
            if mutated {
                cmds.push(Control::Str(format!("{}{}{}", start, content, finish)));
            } else {
                cmds.push(Control::Str(format!("{}{}{}", start, ts.s, finish)));
            }
            for ann in &ts.tag {
                match ann {
                    RichAnnotation::RedactedEnd(_, id) => cmds.push(Control::RedactedEnd(*id)),
                    RichAnnotation::NoBreakEnd => cmds.push(Control::NoBreakEnd),
                    _ => (),
                }
            }
        }

        // html_trace!("YLY: 单元高度:{},单元内容：{:?}",&unit.lines().count(),&unit);
    }

    html_trace!("segments:{:?}", cmds);
    Ok(cmds)
}
