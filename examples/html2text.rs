extern crate argparse;
extern crate html2text;
#[allow(unused_imports)]
use html2text::html_trace;
use argparse::{ArgumentParser, Store, StoreOption, StoreTrue};
use std::io;
use std::io::Read;
use std::io::Result;
use std::io::Write;
use std::slice::Iter;

pub struct StringReader<'a> {
    iter: Iter<'a, u8>,
}

impl<'a> StringReader<'a> {
    /// Wrap a string in a `StringReader`, which implements `std::io::Read`.
    pub fn new(data: &'a str) -> Self {
        Self {
            iter: data.as_bytes().iter(),
        }
    }
}

impl<'a> Read for StringReader<'a> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        for i in 0..buf.len() {
            if let Some(x) = self.iter.next() {
                buf[i] = *x;
            } else {
                return Ok(i);
            }
        }
        Ok(buf.len())
    }
}
#[cfg(feature = "ansi_colours")]
use html2text::render::text_renderer::RichAnnotation;
#[cfg(feature = "ansi_colours")]
use termion;

#[cfg(feature = "ansi_colours")]
fn default_colour_map(
    annotation: &RichAnnotation,
) -> (String, Box<dyn Fn(&String) -> String>, String) {
    use termion::color::*;
    use RichAnnotation::*;
    match annotation {
        Default => ("".into(), Box::new(|s| s.to_string()), "".into()),
        Link(_) => (
            format!("{}", termion::style::Underline),
            Box::new(|s| s.to_string()),
            format!("{}", termion::style::Reset),
        ),
        Image(_,..) => (
            format!("{}", Fg(Blue)),
            Box::new(|s| s.to_string()),
            format!("{}", Fg(Reset)),
        ),
        Emphasis => (
            format!("{}", termion::style::Bold),
            Box::new(|s| s.to_string()),
            format!("{}", termion::style::Reset),
        ),
        Strong => (
            format!("{}", Fg(LightYellow)),
            Box::new(|s| s.to_string()),
            format!("{}", Fg(Reset)),
        ),
        Strikeout => (
            format!("{}", Fg(LightBlack)),
            Box::new(|s| s.to_string()),
            format!("{}", Fg(Reset)),
        ),
        Code => (
            format!("{}", Fg(Blue)),
            Box::new(|s| s.to_string()),
            format!("{}", Fg(Reset)),
        ),
        Preformat(_) => (
            format!("{}", Fg(Blue)),
            Box::new(|s| s.to_string()),
            format!("{}", Fg(Reset)),
        ),
        Colored(c) => (
            (format!(
                "{}",
                Fg(AnsiValue(colvert::ansi256_from_rgb((c.r, c.g, c.b))))
            )),
            Box::new(|s| s.to_string()),
            format!("{}", Fg(Reset)),
        ),
        Bell => todo!(),
        NoBreakBegin => (String::new(), Box::new(|s| s.to_string()), String::new()),
        NoBreakEnd => (String::new(), Box::new(|s| s.to_string()), String::new()),
        RedactedBegin(_, _) => (String::new(), Box::new(|s| s.to_string()), String::new()),
        RedactedEnd(_, _) => (String::new(), Box::new(|s| s.to_string()), String::new()),
    }
}

fn translate<R>(input: R, width: usize, _height: usize,_literall: bool, _use_colour: bool) -> String
where
    R: io::Read,
{
    #[cfg(feature = "ansi_colours")]
    {
        if _use_colour {
            eprintln!("{:#?}",html2text::custom_render(input, width, default_colour_map).unwrap());
            // return process_page(
            //     html2text::from_read_custom(input, width, default_colour_map).unwrap(),
            //     height,
            // );
        };
        return format!("");
    }

}

// fn process_page(segs: Vec<String>, height: usize) -> String {
//     let mut result = String::new();
//     let mut ypos: usize = 0; //relative to 40
//     #[allow(unused_mut)]
//     let mut trace = false;
//     #[cfg(feature = "html_trace")]
//     {
//         trace = true;
//     }
//     for s in segs.iter() {
//         let s = s.as_str();
//         loop {
//             let lines: usize = s.lines().count();
//             if ypos == 0 {
//                 result.push_str(s);
//                 ypos += lines;
//                 ypos = ypos % height;
//                 break;
//             } else {
//                 if ypos + lines >= height {
//                     if trace{
//                         result.push_str("PAD\n".repeat(height - ypos).as_str());
//                     } else {
//                         result.push_str("\n".repeat(height - ypos).as_str());
//                     }
//                     ypos = 0;
//                     continue;
//                 } else {
//                     result.push_str(s);
//                     break;
//                 }
//             }
//         }
//     }
//     result
// }
fn main() {
    let mut infile: Option<String> = None;
    let mut outfile: Option<String> = None;
    let mut width: usize = 80;
    let mut height: usize = 40;
    let mut literal: bool = false;
    #[allow(unused)]
    let mut use_colour = true;

    {
        let mut ap = ArgumentParser::new();
        ap.refer(&mut infile).add_argument(
            "infile",
            StoreOption,
            "Input HTML file (default is standard input)",
        );
        ap.refer(&mut width).add_option(
            &["-w", "--width"],
            Store,
            "Column width to format to (default is 80)",
        );
        ap.refer(&mut height).add_option(
            &["-h", "--height"],
            Store,
            "Terminal height to format to (default is 40)",
        );
        ap.refer(&mut outfile).add_option(
            &["-o", "--output"],
            StoreOption,
            "Output file (default is standard output)",
        );
        ap.refer(&mut literal).add_option(
            &["-L", "--literal"],
            StoreTrue,
            "Output only literal text (no decorations)",
        );
        #[cfg(feature = "ansi_colours")]
        ap.refer(&mut use_colour)
            .add_option(&["--colour"], StoreTrue, "Use ANSI terminal colours");
        ap.parse_args_or_exit();
    }

    let data = match infile {
        None => {
            let stdin = io::stdin();
            let data = translate(&mut stdin.lock(), width, 40, literal, use_colour);
            data
        }
        Some(name) => {
            let mut file = std::fs::File::open(name).expect("Tried to open file");
            translate(&mut file, width, height, literal, use_colour)
        }
    };

    match outfile {
        None => {
            println!("{}", data);
        }
        Some(name) => {
            let mut file = std::fs::File::create(name).expect("Tried to create file");
            write!(file, "{}", data).unwrap();
        }
    };


}
