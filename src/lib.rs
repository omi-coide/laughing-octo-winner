//! Convert HTML to text formats.
//!
//! This crate renders HTML into a text format, wrapped to a specified width.
//! This can either be plain text or with extra annotations to (for example)
//! show in a terminal which supports colours.
//!
//! # Examples
//!
//! ```rust
//! # use html2text::from_read;
//! let html = b"
//!        <ul>
//!          <li>Item one</li>
//!          <li>Item two</li>
//!          <li>Item three</li>
//!        </ul>";
//! assert_eq!(from_read(&html[..], 20),
//!            "\
//! * Item one
//! * Item two
//! * Item three
//! ");
//! ```
//! A couple of simple demonstration programs are included as examples:
//!
//! ### html2text
//!
//! The simplest example uses `from_read` to convert HTML on stdin into plain
//! text:
//!
//! ```sh
//! $ cargo run --example html2text < foo.html
//! [...]
//! ```
//!
//! ### html2term
//!
//! A very simple example of using the rich interface (`from_read_rich`) for a
//! slightly interactive console HTML viewer is provided as `html2term`.
//!
//! ```sh
//! $ cargo run --example html2term foo.html
//! [...]
//! ```
//!
//! Note that this example takes the HTML file as a parameter so that it can
//! read keys from stdin.
//!

#![cfg_attr(feature="clippy", feature(plugin))]
#![cfg_attr(feature="clippy", plugin(clippy))]
#![deny(missing_docs)]

#[macro_use]
extern crate html5ever_atoms;
extern crate html5ever;
extern crate unicode_width;
extern crate backtrace;

#[macro_use]
mod macros;

pub mod render;

use render::Renderer;
use render::text_renderer::{TextRenderer,PlainDecorator,RichDecorator,
                            RichAnnotation,TaggedLine,RenderLine};

use std::io;
use std::io::Write;
use std::cmp::max;
use std::iter::{once,repeat};
use html5ever::{parse_document};
use html5ever::driver::ParseOpts;
use html5ever::tree_builder::TreeBuilderOpts;
use html5ever::rcdom::{self,RcDom,Handle,Text,Element,Document,Comment};
use html5ever::tendril::TendrilSink;

/// A dummy writer which does nothing
struct Discard {}
impl Write for Discard {
    fn write(&mut self, bytes: &[u8]) -> std::result::Result<usize, io::Error> { Ok(bytes.len()) }
    fn flush(&mut self) -> std::result::Result<(), io::Error> { Ok(()) }
}

fn get_text(handle: Handle) -> String {
    let node = handle.borrow();
    let mut result = String::new();
    if let Text(ref tstr) = node.node {
        result.push_str(tstr);
    } else {
        for child in &node.children {
            result.push_str(&get_text(child.clone()));
        }
    }
    result
}

const MIN_WIDTH: usize = 5;

/// Size information/estimate
#[derive(Debug,Copy,Clone)]
pub struct SizeEstimate {
    size: usize,       // Rough overall size
    min_width: usize,  // The narrowest possible
}

impl Default for SizeEstimate {
    fn default() -> SizeEstimate {
        SizeEstimate {
            size: 0,
            min_width: 0,
        }
    }
}

impl SizeEstimate {
    /// Combine two estimates into one (add size and widest required)
    pub fn add(self, other: SizeEstimate) -> SizeEstimate {
        SizeEstimate {
            size: self.size + other.size,
            min_width: max(self.min_width, other.min_width),
        }
    }
}

#[derive(Debug)]
/// Render tree table cell
pub struct RenderTableCell {
    colspan: usize,
    content: Vec<RenderNode>,
    size_estimate: Option<SizeEstimate>,
}

impl RenderTableCell {
    /// Render this cell to a builder.
    pub fn render<T:Write, R:Renderer>(&mut self, builder: &mut R, err_out: &mut T)
    {
        render_tree_children_to_string(builder, &mut self.content, err_out)
    }

    /// Calculate or return the estimate size of the cell
    pub fn get_size_estimate(&mut self) -> SizeEstimate {
        if self.size_estimate.is_none() {
            let size = self.content
                           .iter_mut()
                           .map(|node| node.get_size_estimate())
                           .fold(Default::default(), SizeEstimate::add);
            self.size_estimate = Some(size);
        }
        self.size_estimate.unwrap()
    }
}

#[derive(Debug)]
/// Render tree table row
pub struct RenderTableRow {
    cells: Vec<RenderTableCell>,
}

impl RenderTableRow {
    /// Return a mutable iterator over the cells.
    pub fn cells(&mut self) -> std::slice::IterMut<RenderTableCell> {
        self.cells.iter_mut()
    }
    /// Count the number of cells in the row.
    /// Takes into account colspan.
    pub fn num_cells(&self) -> usize {
        self.cells.iter().map(|cell| cell.colspan).sum()
    }
    /// Return an iterator over (column, &cell)s, which
    /// takes into account colspan.
    pub fn cell_columns(&mut self) -> Vec<(usize, &mut RenderTableCell)> {
        let mut result = Vec::new();
        let mut colno = 0;
        for cell in &mut self.cells {
            let colspan = cell.colspan;
            result.push((colno, cell));
            colno += colspan;
        }
        result
    }
}

#[derive(Debug)]
/// A representation of a table render tree with metadata.
pub struct RenderTable {
    rows: Vec<RenderTableRow>,
    num_columns: usize,
    size_estimate: Option<SizeEstimate>,
}

impl RenderTable {
    /// Create a new RenderTable with the given rows
    pub fn new(rows: Vec<RenderTableRow>) -> RenderTable {
        let num_columns = rows.iter()
                              .map(|r| r.num_cells()).max().unwrap_or(0);
        RenderTable {
            rows: rows,
            num_columns: num_columns,
            size_estimate: None,
        }
    }

    /// Return an iterator over the rows.
    pub fn rows(&mut self) -> std::slice::IterMut<RenderTableRow> {
        self.rows.iter_mut()
    }

    fn calc_size_estimate(&mut self) {
        let mut sizes: Vec<SizeEstimate> = vec![Default::default(); self.num_columns];

        // For now, a simple estimate based on adding up sub-parts.
        for row in self.rows() {
            let mut colno = 0usize;
            for cell in row.cells() {
                let cellsize = cell.get_size_estimate();
                for colnum in 0..cell.colspan {
                    sizes[colno + colnum].size += cellsize.size / cell.colspan;
                    sizes[colno + colnum].min_width = max(sizes[colno+colnum].min_width/cell.colspan, cellsize.min_width);
                }
                colno += cell.colspan;
            }
        }
        let size = sizes.iter().map(|s| s.size).sum();  // Include borders?
        let min_width = sizes.iter().map(|s| s.min_width).sum::<usize>() + self.num_columns-1;
        self.size_estimate = Some(SizeEstimate { size: size, min_width: min_width });
    }

    /// Calculate and store (or return stored value) of estimated size
    pub fn get_size_estimate(&mut self) -> SizeEstimate {
        if self.size_estimate.is_none() {
            self.calc_size_estimate();
        }
        self.size_estimate.unwrap()
    }
}

/// The node-specific information distilled from the DOM.
#[derive(Debug)]
pub enum RenderNodeInfo {
    /// Some text.
    Text(String),
    /// A group of nodes collected together.
    Container(Vec<RenderNode>),
    /// A link with contained nodes
    Link(String, Vec<RenderNode>),
    /// An emphasised region
    Em(Vec<RenderNode>),
    /// A code region
    Code(Vec<RenderNode>),
    /// An image (title)
    Img(String),
    /// A block element with children
    Block(Vec<RenderNode>),
    /// A Div element with children
    Div(Vec<RenderNode>),
    /// A preformatted region.
    Pre(String),
    /// A blockquote
    BlockQuote(Vec<RenderNode>),
    /// An unordered list
    Ul(Vec<RenderNode>),
    /// An ordered list
    Ol(Vec<RenderNode>),
    /// A line break
    Break,
    /// A table
    Table(RenderTable),
}

/// Common fields from a node.
#[derive(Debug)]
pub struct RenderNode {
    size_estimate: Option<SizeEstimate>,
    info: RenderNodeInfo,
}

impl RenderNode {
    /// Create a node from the RenderNodeInfo.
    pub fn new(info: RenderNodeInfo) -> RenderNode {
        RenderNode {
            size_estimate: None,
            info: info,
        }
    }

    /// Get a size estimate (~characters)
    pub fn get_size_estimate(&mut self) -> SizeEstimate {
        // If it's already calculated, then just return the answer.
        if let Some(s) = self.size_estimate {
            return s;
        };

        use RenderNodeInfo::*;

        // Otherwise, make an estimate.
        let estimate = match self.info {
            Text(ref t) |
            Img(ref t) |
            Pre(ref t) => SizeEstimate { size: t.len(), min_width: MIN_WIDTH },

            Container(ref mut v) |
            Link(_, ref mut v) |
            Em(ref mut v) |
            Code(ref mut v) |
            Block(ref mut v) |
            Div(ref mut v) |
            BlockQuote(ref mut v) |
            Ul(ref mut v) |
            Ol(ref mut v) => {
                v.iter_mut()
                 .map(RenderNode::get_size_estimate)
                 .fold(Default::default(), SizeEstimate::add)
            },
            Break => SizeEstimate { size: 1, min_width: 1 },
            Table(ref mut t) => {
                t.get_size_estimate()
            },
        };
        self.size_estimate = Some(estimate);
        estimate
    }
}

/// Make a Vec of RenderNodes from the children of a node.
fn children_to_render_nodes<T:Write>(handle: Handle, err_out: &mut T) -> Vec<RenderNode> {
    /* process children, but don't add anything */
    let children = handle.borrow().children
                                  .iter()
                                  .flat_map(|ch| dom_to_render_tree(ch.clone(), err_out))
                                  .collect();
    children
}

/// Make a Vec of RenderNodes from the <li>children of a node.
fn list_children_to_render_nodes<T:Write>(handle: Handle, err_out: &mut T) -> Vec<RenderNode> {
    let node = handle.borrow();
    let mut children = Vec::new();

    for child in &node.children {
        match child.borrow().node {
            Element(ref name, _, _) => {
                match *name {
                    qualname!(html, "li") => {
                        let li_children = children_to_render_nodes(child.clone(), err_out);
                        children.push(RenderNode::new(RenderNodeInfo::Block(li_children)));
                    },
                    _ => {},
                }
            },
            Comment(_) => {},
            _ => { html_trace!("Unhandled in list: {:?}\n", child); },
        }
    }
    children
}

/// Convert a table into a RenderNode
fn table_to_render_tree<T:Write>(handle: Handle, err_out: &mut T) -> Option<RenderNode> {
    let node = handle.borrow();

    for child in &node.children {
        match child.borrow().node {
            Element(ref name, _, _) => {
                match *name {
                    qualname!(html, "tbody") => return tbody_to_render_tree(child.clone(), err_out),
                    _ => { writeln!(err_out, "  [[table child: {:?}]]", name).unwrap(); },
                }
            },
            Comment(_) => {},
            _ => { html_trace!("Unhandled in table: {:?}\n", child); },
        }
    }
    None
}

/// Convert the tbody element to a RenderNode.
fn tbody_to_render_tree<T:Write>(handle: Handle, err_out: &mut T) -> Option<RenderNode> {
    let node = handle.borrow();

    let mut rows = Vec::new();

    for child in &node.children {
        match child.borrow().node {
            Element(ref name, _, _) => {
                match *name {
                    qualname!(html, "tr") => {
                        rows.push(tr_to_render_tree(child.clone(), err_out));
                    },
                    _ => { html_trace!("  [[tbody child: {:?}]]", name); },
                }
            },
            Comment(_) => {},
            _ => { html_trace!("Unhandled in tbody: {:?}\n", child); },
        }
    }
    if rows.len() > 0 {
        Some(RenderNode::new(RenderNodeInfo::Table(RenderTable::new(rows))))
    } else {
        None
    }
}

/// Convert a table row to a RenderTableRow
fn tr_to_render_tree<T:Write>(handle: Handle, err_out: &mut T) -> RenderTableRow {
    let node = handle.borrow();

    let mut cells = Vec::new();

    for child in &node.children {
        match child.borrow().node {
            Element(ref name, _, _) => {
                match *name {
                    qualname!(html, "th") |
                    qualname!(html, "td") => {
                        cells.push(td_to_render_tree(child.clone(), err_out));
                    },
                    _ => { html_trace!("  [[tr child: {:?}]]", name); },
                }
            },
            Comment(_) => {},
            _ => { html_trace!("Unhandled in tr: {:?}\n", child); },
        }
    }

    RenderTableRow {
        cells: cells,
    }
}

/// Convert a single table cell to a render node.
fn td_to_render_tree<T: Write>(handle: Handle, err_out: &mut T) -> RenderTableCell {
    let children = children_to_render_nodes(handle.clone(), err_out);
    let mut colspan = 1;
    if let Element(_, _, ref attrs) = handle.borrow().node {
        for attr in attrs {
            if &attr.name.local == "colspan" {
                let v:&str = &*attr.value;
                colspan = v.parse().unwrap_or(1);
            }
        }
    }
    RenderTableCell {
        colspan: colspan,
        content: children,
        size_estimate: None,
    }
}


/// Convert a DOM tree or subtree into a render tree.
pub fn dom_to_render_tree<T:Write>(handle: Handle, err_out: &mut T) -> Option<RenderNode> {
    use RenderNodeInfo::*;
    let node = handle.borrow();
    let result = match node.node {
        Document => Some(RenderNode::new(Container(children_to_render_nodes(handle.clone(), err_out)))),
        Comment(_) => None,
        Element(ref name, _, ref attrs) => {
            match *name {
                qualname!(html, "html") |
                qualname!(html, "span") |
                qualname!(html, "body") => {
                    /* process children, but don't add anything */
                    Some(RenderNode::new(Container(children_to_render_nodes(handle.clone(), err_out))))
                },
                qualname!(html, "link") |
                qualname!(html, "meta") |
                qualname!(html, "hr") |
                qualname!(html, "script") |
                qualname!(html, "style") |
                qualname!(html, "head") => {
                    /* Ignore the head and its children */
                    None
                },
                qualname!(html, "a") => {
                    let mut target = None;
                    for attr in attrs {
                        if &attr.name.local == "href" {
                            target = Some(&*attr.value);
                            break;
                        }
                    }
                    let children = children_to_render_nodes(handle.clone(), err_out);
                    if let Some(href) = target {
                        Some(RenderNode::new(Link(href.into(), children)))
                    } else {
                        Some(RenderNode::new(Container(children)))
                    }
                },
                qualname!(html, "em") => {
                    Some(RenderNode::new(Em(children_to_render_nodes(handle.clone(), err_out))))
                },
                qualname!(html, "code") => {
                    Some(RenderNode::new(Code(children_to_render_nodes(handle.clone(), err_out))))
                },
                qualname!(html, "img") => {
                    let mut title = None;
                    for attr in attrs {
                        if &attr.name.local == "alt" {
                            title = Some(&*attr.value);
                            break;
                        }
                    }
                    if let Some(title) = title {
                        Some(RenderNode::new(Img(title.into())))
                    } else {
                        None
                    }
                },
                qualname!(html, "h1") |
                qualname!(html, "h2") |
                qualname!(html, "h3") |
                qualname!(html, "h4") |
                qualname!(html, "p") => {
                    Some(RenderNode::new(Block(children_to_render_nodes(handle.clone(), err_out))))
                },
                qualname!(html, "div") => {
                    Some(RenderNode::new(Div(children_to_render_nodes(handle.clone(), err_out))))
                },
                qualname!(html, "pre") => {
                    Some(RenderNode::new(Pre(get_text(handle.clone()))))
                },
                qualname!(html, "br") => {
                    Some(RenderNode::new(Break))
                }
                qualname!(html, "table") => table_to_render_tree(handle.clone(), err_out),
                qualname!(html, "blockquote") => {
                    Some(RenderNode::new(BlockQuote(children_to_render_nodes(handle.clone(), err_out))))
                },
                qualname!(html, "ul") => {
                    Some(RenderNode::new(Ul(list_children_to_render_nodes(handle.clone(), err_out))))
                },
                qualname!(html, "ol") => {
                    Some(RenderNode::new(Ol(list_children_to_render_nodes(handle.clone(), err_out))))
                },
                _ => {
                    html_trace!("Unhandled element: {:?}\n", name.local);
                    Some(RenderNode::new(Container(children_to_render_nodes(handle.clone(), err_out))))
                    //None
                },
            }
          },
        rcdom::Text(ref tstr) => {
            Some(RenderNode::new(Text(tstr.into())))
        }
        _ => { write!(err_out, "Unhandled: {:?}\n", node).unwrap(); None },
    };
    html_trace!("### dom_to_render_tree: HTML: {:?}", node);
    html_trace!("### dom_to_render_tree: out= {:#?}", result);
    return result;
}

fn render_tree_children_to_string<T:Write, R:Renderer>(builder: &mut R,
                                                      children: &mut Vec<RenderNode>,
                                                      err_out: &mut T) {
    for child in children {
        render_tree_to_string(builder, child, err_out);
    }
}

fn render_tree_to_string<T:Write, R:Renderer>(builder: &mut R, tree: &mut RenderNode,
                          err_out: &mut T) {
    use RenderNodeInfo::*;
    match tree.info {
        Text(ref tstr) => {
            builder.add_inline_text(tstr);
        },
        Container(ref mut children) => {
            render_tree_children_to_string(builder, children, err_out);
        },
        Link(ref href, ref mut children) => {
            builder.start_link(href);
            render_tree_children_to_string(builder, children, err_out);
            builder.end_link();
        },
        Em(ref mut children) => {
            builder.start_emphasis();
            render_tree_children_to_string(builder, children, err_out);
            builder.end_emphasis();
        },
        Code(ref mut children) => {
            builder.start_code();
            render_tree_children_to_string(builder, children, err_out);
            builder.end_code();
        },
        Img(ref title) => {
            builder.add_image(title);
        },
        Block(ref mut children) => {
            builder.start_block();
            render_tree_children_to_string(builder, children, err_out);
            builder.end_block();
        },
        Div(ref mut children) => {
            builder.new_line();
            render_tree_children_to_string(builder, children, err_out);
            builder.new_line();
        },
        Pre(ref formatted) => {
            builder.add_preformatted_block(formatted);
        },
        BlockQuote(ref mut children) => {
            let mut sub_builder = builder.new_sub_renderer(builder.width()-2);
            render_tree_children_to_string(&mut sub_builder, children, err_out);

            builder.start_block();
            builder.append_subrender(sub_builder, repeat("> "));
            builder.end_block();
        },
        Ul(ref mut items) => {
            builder.start_block();
            for item in items {
                let mut sub_builder = builder.new_sub_renderer(builder.width()-2);
                render_tree_to_string(&mut sub_builder, item, err_out);
                builder.append_subrender(sub_builder, once("* ").chain(repeat("  ")));
            }
        },
        Ol(ref mut items) => {
            let num_items = items.len();

            builder.start_block();

            let prefix_width = format!("{}", num_items).len() + 2;

            let mut i = 1;
            let prefixn = format!("{: <width$}", "", width=prefix_width);
            for item in items {
                let mut sub_builder = builder.new_sub_renderer(builder.width()-prefix_width);
                render_tree_to_string(&mut sub_builder, item, err_out);
                let prefix1 = format!("{}.", i);
                let prefix1 = format!("{: <width$}", prefix1, width=prefix_width);
                builder.append_subrender(sub_builder, once(prefix1.as_str()).chain(repeat(prefixn.as_str())));
                i += 1;
            }
        },
        Break => {
            builder.new_line();
        },
        Table(ref mut tab) => {
            render_table_tree(builder, tab, err_out);
        },
    }
}

fn render_table_tree<T:Write, R:Renderer>(builder: &mut R, table: &mut RenderTable, err_out: &mut T) {
    /* Now lay out the table. */
    let num_columns = table.num_columns;

    /* Heuristic: scale the column widths according to how much content there is. */
    let mut col_sizes: Vec<SizeEstimate> = vec![Default::default(); num_columns];

    for row in table.rows() {
        let mut colno = 0;
        for cell in row.cells() {
            let mut estimate = cell.get_size_estimate();
            // If the cell has a colspan>1, then spread its size between the
            // columns.
            estimate.size /= cell.colspan;
            estimate.min_width /= cell.colspan;
            for i in 0..cell.colspan {
                col_sizes[colno + i] = (col_sizes[colno + i]).add(estimate);
            }
            colno += cell.colspan;
        }
    }
    let tot_size: usize = col_sizes.iter().map(|est| est.size).sum();
    let width = builder.width();
    let mut col_widths:Vec<usize> = col_sizes.iter()
                                         .map(|sz| {
                                             if sz.size == 0 {
                                                 0
                                             } else {
                                                 max(sz.size * width / tot_size, sz.min_width)
                                             }
                                          }).collect();
    /* The minimums may have put the total width too high */
    while col_widths.iter().cloned().sum::<usize>() > width {
        let (i, _) = col_widths.iter()
                               .cloned()
                               .enumerate()
                               .max_by_key(|&(colno, width)| (width.saturating_sub(col_sizes[colno].min_width), width, usize::max_value() - colno ))
                               .unwrap();
        col_widths[i] -= 1;
    }
    if !col_widths.is_empty() {
        // Slight fudge; we're not drawing extreme edges, so one of the columns
        // can gets a free character cell from not having a border.
        // make it the last.
        let last = col_widths.len() - 1;
        col_widths[last] += 1;
    }

    builder.start_block();

    builder.add_horizontal_border();

    for row in table.rows() {
        let rendered_cells: Vec<R::Sub> = row.cell_columns()
                                             .into_iter()
                                             .flat_map(|(colno, cell)| {
                                                  let col_width:usize = col_widths[colno..colno+cell.colspan]
                                                                     .iter().sum();
                                                  if col_width > 1 {
                                                      let mut cellbuilder = builder.new_sub_renderer(col_width-1);
                                                      cell.render(&mut cellbuilder, err_out);
                                                      Some(cellbuilder)
                                                  } else {
                                                      None
                                                  }
                                              }).collect();
        if rendered_cells.iter().any(|r| !r.empty()) {
            builder.append_columns_with_borders(rendered_cells, true);
        }
    }
}

/// Reads HTML from `input`, and returns a `String` with text wrapped to
/// `width` columns.
pub fn from_read<R>(mut input: R, width: usize) -> String where R: io::Read {
    let opts = ParseOpts {
        tree_builder: TreeBuilderOpts {
            drop_doctype: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let dom = parse_document(RcDom::default(), opts)
                   .from_utf8()
                   .read_from(&mut input)
                   .unwrap();

    let decorator = PlainDecorator::new();
    let mut builder = TextRenderer::new(width, decorator);

    let mut render_tree = dom_to_render_tree(dom.document, &mut Discard{}).unwrap();
    render_tree_to_string(&mut builder, &mut render_tree, &mut Discard{});
    builder.into_string()
}

/// Reads HTML from `input`, and returns text wrapped to `width` columns.
/// The text is returned as a `Vec<TaggedLine<_>>`; the annotations are vectors
/// of `RichAnnotation`.  The "outer" annotation comes first in the `Vec`.
pub fn from_read_rich<R>(mut input: R, width: usize) -> Vec<TaggedLine<Vec<RichAnnotation>>>
        where R: io::Read
{
    let opts = ParseOpts {
        tree_builder: TreeBuilderOpts {
            drop_doctype: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let dom = parse_document(RcDom::default(), opts)
                   .from_utf8()
                   .read_from(&mut input)
                   .unwrap();

    let decorator = RichDecorator::new();
    let mut builder = TextRenderer::new(width, decorator);
    let mut render_tree = dom_to_render_tree(dom.document, &mut Discard{}).unwrap();
    render_tree_to_string(&mut builder, &mut render_tree, &mut Discard{});
    builder.into_lines().into_iter().map(RenderLine::into_tagged_line).collect()
}

#[cfg(test)]
mod tests {
    use super::{from_read};

    /// Like assert_eq!(), but prints out the results normally as well
    macro_rules! assert_eq_str {
        ($a:expr, $b:expr) => {
            if $a != $b {
                println!("<<<\n{}===\n{}>>>", $a, $b);
                assert_eq!($a, $b);
            }
        }
    }
    fn test_html(input: &[u8], expected: &str, width: usize) {
        assert_eq_str!(from_read(input, width), expected);
    }

    #[test]
    fn test_table() {
        test_html(br##"
       <table>
         <tr>
           <td>1</td>
           <td>2</td>
           <td>3</td>
         </tr>
       </table>
"##, r#"───┬───┬────
1  │2  │3   
───┴───┴────
"#, 12);
     }

     #[test]
     fn test_colspan() {
        test_html(br##"
       <table>
         <tr>
           <td>1</td>
           <td>2</td>
           <td>3</td>
         </tr>
         <tr>
           <td colspan="2">12</td>
           <td>3</td>
         </tr>
         <tr>
           <td>1</td>
           <td colspan="2">23</td>
         </tr>
       </table>
"##, r#"───┬───┬────
1  │2  │3   
───┴───┼────
12     │3   
───┬───┴────
1  │23      
───┴────────
"#, 12);
     }

     #[test]
     fn test_para() {
        assert_eq_str!(from_read(&b"<p>Hello</p>"[..], 10),
                   "Hello\n");
     }

     #[test]
     fn test_para2() {
        assert_eq_str!(from_read(&b"<p>Hello, world!</p>"[..], 20),
                   "Hello, world!\n");
     }

     #[test]
     fn test_blockquote() {
        assert_eq_str!(from_read(&br#"<p>Hello</p>
        <blockquote>One, two, three</blockquote>
        <p>foo</p>
"#[..], 12), r#"Hello

> One, two,
> three

foo
"#);
     }

     #[test]
     fn test_ul() {
         test_html(br#"
            <ul>
              <li>Item one</li>
              <li>Item two</li>
              <li>Item three</li>
            </ul>
         "#, r#"* Item one
* Item two
* Item
  three
"#, 10);
     }

     #[test]
     fn test_strip_nl() {
         test_html(br#"
            <p>
               One
               Two
               Three
            </p>
         "#, "One Two Three\n", 40);
     }
     #[test]
     fn test_strip_nl2() {
         test_html(br#"
            <p>
               One
               <span>
                   Two
               </span>
               Three
            </p>
         "#, "One Two Three\n", 40);
     }
     #[test]
     fn test_strip_nl_tbl() {
         test_html(br#"
           <table>
             <tr>
                <td>
                   One
                   <span>
                       Two
                   </span>
                   Three
                </td>
              </tr>
            </table>
         "#, r"────────────────────
One Two Three       
────────────────────
", 20);
     }
     #[test]
     fn test_unknown_element() {
         test_html(br#"
           <foo>
           <table>
             <tr>
                <td>
                   One
                   <span><yyy>
                       Two
                   </yyy></span>
                   Three
                </td>
              </tr>
            </table>
            </foo>
         "#, r"────────────────────
One Two Three       
────────────────────
", 20);
     }
     #[test]
     fn test_strip_nl_tbl_p() {
         test_html(br#"
           <table>
             <tr>
                <td><p>
                   One
                   <span>
                       Two
                   </span>
                   Three
                </p></td>
              </tr>
            </table>
         "#, r"────────────────────
One Two Three       
────────────────────
", 20);
     }
     #[test]
     fn test_pre() {
         test_html(br#"
           <pre>foo
    bar
  wib   asdf;
</pre>
<p>Hello</p>
         "#, r"foo
    bar
  wib   asdf;

Hello
", 20);
    }
     #[test]
     fn test_link() {
         test_html(br#"
           <p>Hello, <a href="http://www.example.com/">world</a></p>"#, r"Hello, [world][1]

[1] http://www.example.com/
", 80);
    }
     #[test]
     fn test_link2() {
         test_html(br#"
           <p>Hello, <a href="http://www.example.com/">world</a>!</p>"#, r"Hello, [world][1]!

[1] http://www.example.com/
", 80);
     }

     #[test]
     fn test_link3() {
         test_html(br#"
           <p>Hello, <a href="http://www.example.com/">w</a>orld</p>"#, r"Hello, [w][1]orld

[1] http://www.example.com/
", 80);
     }

     #[test]
     fn test_link_wrap() {
         test_html(br#"
           <a href="http://www.example.com/">Hello</a>"#, r"[Hello][1]

[1] http:/
/www.examp
le.com/
", 10);
     }

     #[test]
     fn test_wrap() {
         test_html(br"<p>Hello, world.  Superlongwordreally</p>",
                   r#"Hello,
world.
Superlon
gwordrea
lly
"#, 8);
     }

     #[test]
     fn test_wrap2() {
         test_html(br"<p>Hello, world.  This is a long sentence with a
few words, which we want to be wrapped correctly.</p>",
r#"Hello, world. This
is a long sentence
with a few words,
which we want to be
wrapped correctly.
"#, 20);
     }

     #[test]
     fn test_wrap3() {
         test_html(br#"<p><a href="dest">http://example.org/blah/</a> one two three"#,
r#"[http://example.org/blah/
][1] one two three

[1] dest
"#, 25);
     }

     #[test]
     fn test_div() {
         test_html(br"<p>Hello</p><div>Div</div>",
r#"Hello

Div
"#, 20);
         test_html(br"<p>Hello</p><div>Div</div><div>Div2</div>",
r#"Hello

Div
Div2
"#, 20);
     }

     #[test]
     fn test_img_alt() {
         test_html(br"<p>Hello <img src='foo.jpg' alt='world'></p>",
                   "Hello [world]\n", 80);
     }

     #[test]
     fn test_br() {
         test_html(br"<p>Hello<br/>World</p>",
                   "Hello\nWorld\n", 20);
     }

     #[test]
     fn test_subblock() {
         test_html(br#"<div>
         <div>Here's a <a href="https://example.com/">link</a>.</div>
         <div><ul>
         <li>Bullet</li>
         <li>Bullet</li>
         <li>Bullet</li>
         </ul></div>
         </div>"#,
r"Here's a [link][1].

* Bullet
* Bullet
* Bullet

[1] https://example.com/
", 80);
     }

     #[test]
     fn test_controlchar() {
         test_html("Foo\u{0080}Bar".as_bytes(), "FooBar\n", 80);
         test_html("Foo\u{0080}Bar".as_bytes(), "FooB\nar\n", 4);
         test_html("FooBa\u{0080}r".as_bytes(), "FooB\nar\n", 4);
     }

     #[test]
     fn test_nested_table_1() {
        test_html(br##"
       <table>
         <tr>
           <td>
              <table><tr><td>1</td><td>2</td><td>3</td></tr></table>
           </td>
           <td>
              <table><tr><td>4</td><td>5</td><td>6</td></tr></table>
           </td>
           <td>
              <table><tr><td>7</td><td>8</td><td>9</td></tr></table>
           </td>
         </tr>
         <tr>
           <td>
              <table><tr><td>1</td><td>2</td><td>3</td></tr></table>
           </td>
           <td>
              <table><tr><td>4</td><td>5</td><td>6</td></tr></table>
           </td>
           <td>
              <table><tr><td>7</td><td>8</td><td>9</td></tr></table>
           </td>
         </tr>
         <tr>
           <td>
              <table><tr><td>1</td><td>2</td><td>3</td></tr></table>
           </td>
           <td>
              <table><tr><td>4</td><td>5</td><td>6</td></tr></table>
           </td>
           <td>
              <table><tr><td>7</td><td>8</td><td>9</td></tr></table>
           </td>
         </tr>
       </table>
"##, r#"─┬─┬──┬─┬─┬──┬─┬─┬───
1│2│3 │4│5│6 │7│8│9  
─┼─┼──┼─┼─┼──┼─┼─┼───
1│2│3 │4│5│6 │7│8│9  
─┼─┼──┼─┼─┼──┼─┼─┼───
1│2│3 │4│5│6 │7│8│9  
─┴─┴──┴─┴─┴──┴─┴─┴───
"#, 21);
     }
}
