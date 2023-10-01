//! Convenience helper for producing coloured terminal output.
//!
//! This optional helper applies terminal colours (or other effects which
//! can be achieved using inline characters sent to the terminal such as
//! underlining in some terminals).


use crate::{parse, RichAnnotation, RichDecorator};
use std::fmt::Write;
use std::io;

/// Reads HTML from `input`, and returns text wrapped to `width` columns.
/// The text is returned as a `Vec<TaggedLine<_>>`; the annotations are vectors
/// of `RichAnnotation`.  The "outer" annotation comes first in the `Vec`.
///
/// The function `colour_map` is given a slice of `RichAnnotation` and should
/// return a pair of static strings which should be inserted before/after a text
/// span with that annotation; for example a string which sets text colour
/// and a string which sets the colour back to the default.
pub fn from_read_coloured<R, FMap>(
    input: R,
    width: usize,
    colour_map: FMap,
) -> Result<String, std::fmt::Error>
where
    R: io::Read,
    FMap: Fn(&RichAnnotation) -> (String, String),
{
    let lines = parse(input)
        .render(width, RichDecorator::new())
        .into_lines();

    let mut result = String::new();
    for line in lines {
        for ts in line.tagged_strings() {
            let mut start = String::new();
            let mut finish = String::new();
            for ann in &ts.tag {
                let (s, f) = colour_map(ann);
                start.push_str(&s);
                finish.push_str(&f);
            }
            write!(result, "{}{}{}", start, ts.s, finish)?;
        }
        result.push('\n');
    }
    Ok(result)
}

/// same as from_read_coloured, but can transform content string
pub fn from_read_custom<R, FMap>(
    input: R,
    width: usize,
    map: FMap,
) -> Result<Vec<String>, std::fmt::Error>
where
    R: io::Read,
    FMap: Fn(&RichAnnotation) -> (String, Box<dyn Fn(&String)->String>, String),
{
    let lines = parse(input)
        .render(width, RichDecorator::new())
        .into_lines();
    let mut segments: Vec<String> = Vec::new();
    let mut result = String::new();
    let mut is_very_beginning = true;
    html_trace!("循环开始");
    for line in lines {
        let mut breaked = false;
        for ts in line.tagged_strings() {
            let mut start = String::new();
            let mut finish = String::new();
            let mut content = String::new();
            let mut mutated = false;
            if ts.tag.contains(&RichAnnotation::NoBreakBegin) {
                    // assert!()
                    if !is_very_beginning {
                        breaked = true;
                    }
                    segments.push(result.clone());
                    result.clear();
            }
            for ann in &ts.tag {
                mutated = true;
                let (s, mutator, f) = map(ann);
                start.push_str(&s);
                finish.push_str(&f);
                html_trace!("变化前:{}",&ts.s);
                html_trace!("变化后:{}",mutator(&ts.s));
                content.push_str(&mutator(&ts.s));
            }
            if mutated {
                write!(result, "{}{}{}", start, content, finish)?;
                is_very_beginning=false;
            } else {
                write!(result, "{}{}{}", start, ts.s, finish)?;
                is_very_beginning=false;
            }
            if ts.tag.contains(&RichAnnotation::NoBreakEnd) {
                // assert!()
                breaked = true;
                segments.push(result.clone());
                result.clear();
            }
        }
        if !breaked {
            result.push('\n');
        }
        html_trace!("YLY: 单元高度:{},单元内容：{:?}",&unit.lines().count(),&unit);
    }
    if !result.is_empty() {
        segments.push(result);
    }
    Ok(segments)
}
