use std::error::Error;
use std::collections::HashMap;
use nom::{IResult,
    number::complete::{be_u8, le_u8, be_i32, le_u32},
};
use tuple::{TupleElements};
use pathfinder_geometry::transform2d::Transform2F;
use itertools::Itertools;
use indexmap::IndexMap;
use crate::{Font, BorrowedFont, Glyph, State, v, R, IResultExt, Context};
use crate::postscript::{Vm, RefItem};
use crate::eexec::Decoder;

pub struct Type1Font {
    vm: Vm,
    glyphs: IndexMap<u32, Glyph>, // codepoint -> glyph
    names: HashMap<String, u32>, // name -> glyph id
    font_matrix: Transform2F
}
impl Font for Type1Font {
    fn num_glyphs(&self) -> u32 {
        self.glyphs.len() as u32
    }
    fn font_matrix(&self) -> Transform2F {
        self.font_matrix
    }
    fn glyph(&self, id: u32) -> Result<Glyph, Box<dyn Error>> {
        match self.glyphs.get_index(id as usize) {
            Some((_, glyphs)) => Ok(glyphs.clone()),
            None => Err("out of bounds".into())
        }
    }
    fn gid_for_codepoint(&self, codepoint: u32) -> Option<u32> {
        self.glyphs.get_full(&codepoint).map(|(index, _, _)| index as u32)
    }
    fn gid_for_name(&self, name: &str) -> Option<u32> {
        self.names.get(name).cloned()
    }
}
impl<'a> BorrowedFont<'a> for Type1Font {}

impl Type1Font {
    pub fn parse_pfb(data: &[u8]) -> Self {
        let mut vm = Vm::new();
        parse_pfb(&mut vm, data).get();
        Self::from_vm(vm)
    }
    pub fn parse_postscript(data: &[u8]) -> Self {
        let mut vm = Vm::new();
        vm.parse_and_exec(data);
        Self::from_vm(vm)
    }
        
    pub fn from_vm(vm: Vm) -> Self {
        let (font_name, font_dict) = vm.fonts().nth(0).unwrap();
        
        let private_dict = font_dict.get("Private").unwrap()
            .as_dict().unwrap();
        let len_iv = private_dict.get("lenIV")
            .map(|i| i.as_int().unwrap()).unwrap_or(4) as usize;
        
        debug!("FontDict keys: {:?}", font_dict.iter().map(|(k, v)| k).format(", "));
        debug!("Private keys: {:?}", private_dict.iter().map(|(k, v)| k).format(", "));
        
        let char_strings = font_dict.get("CharStrings").unwrap().as_dict().unwrap();
        
        let subrs = private_dict.get("Subrs").unwrap()
            .as_array().unwrap().iter()
            .map(|item| Decoder::charstring().decode(item.as_bytes().unwrap(), len_iv).into())
            .collect();
        
        let context = Context {
            subr_bias: 0,
            subrs,
            global_subr_bias: 0,
            global_subrs: vec![]
        };

        let encoding = font_dict.get("Encoding").unwrap().as_array().unwrap();
        
        let mut names = HashMap::new();
        let mut glyphs = IndexMap::new();
        let mut codepoint = 0;
        for item in encoding.iter() {
            match item {
                RefItem::Null => {},
                RefItem::Literal(b".notdef") => {},
                RefItem::Literal(name) => {
                    let name = std::str::from_utf8(name).unwrap();
                    let char_string = char_strings.get(name).unwrap().as_bytes().unwrap();
                    
                    let decoded = Decoder::charstring().decode(&char_string, len_iv);
                    //debug!("{} decoded: {:?}", name, String::from_utf8_lossy(&decoded));
                    
                    let mut state = State::new();
                    charstring(&decoded, &context, &mut state).unwrap();
                    
                    let (index, _) = glyphs.insert_full(codepoint, Glyph {
                        path: state.path,
                        width: state.char_width.unwrap()
                    });
                    names.insert(name.to_owned(), index as u32);
                }
                _ => {}
            }
            codepoint += 1;
        }
        let font_matrix = font_dict.get("FontMatrix").unwrap().as_array().unwrap();
        assert_eq!(font_matrix.len(), 6);
        
        let (a, b, c, d, e, f) = TupleElements::from_iter(
                font_matrix.iter().map(|item| item.as_f32().unwrap())
        ).unwrap();
        
        Type1Font {
            font_matrix: Transform2F::row_major(a, b, c, d, e, f),
            glyphs,
            names,
            vm
        }
    }
}

fn parse_binary<'a>(vm: &mut Vm, data: &'a [u8]) {
    let mut decoder = Decoder::file();
    let decoded = decoder.decode(data, 4);
    
    vm.parse_and_exec(&decoded)
}

#[test]
fn test_parser() {
    use crate::IResultExt;
    let mut vm = Vm::new();
    vm.parse_and_exec(b"/FontBBox{-180 -293 1090 1010}readonly ");
    vm.print_stack();
    assert_eq!(vm.stack().len(), 2);
}
fn parse_pfb<'a>(mut vm: &mut Vm, i: &'a [u8]) -> R<'a, ()> {
    let mut input = i;
    while input.len() > 0 {
        let (i, magic) = le_u8(input)?;
        assert_eq!(magic, 0x80);
        let (i, block_type) = le_u8(i)?;
        match block_type {
            1 | 2 => {} // continue
            3 => break,
            n => panic!("unknown block type {}", n)
        }
        
        let (i, block_len) = le_u32(i)?;
        info!("block type {}, length: {}", block_type, block_len);
    
        let block = &i[.. block_len as usize];
        match block_type {
            1 => vm.parse_and_exec(block),
            2 => parse_binary(&mut vm, block),
            _ => unreachable!()
        }
        
        input = &i[block_len as usize ..];
    }
    
    Ok((input, ()))
}
pub fn charstring<'a, 'b>(mut input: &'a [u8], ctx: &'a Context<'a>, s: &'b mut State) -> IResult<&'a [u8], ()> {
    while input.len() > 0 {
        let (i, b0) = be_u8(input)?;
        let i = match b0 {
            1 => { // ⊦ y dy hstem (1) ⊦
                debug!("hstem");
                s.stack.clear();
                i
            }
            3 => { // ⊦ x dx vstem (3) ⊦
                debug!("vstem");
                s.stack.clear();
                i
            }
            4 => { // ⊦ dy vmoveto (4) ⊦
                debug!("vmoveto");
                let p = s.current + v(0., s.stack[0]);
                s.path.move_to(p);
                s.stack.clear();
                s.current = p;
                i
            }
            5 => { // ⊦ dx dy rlineto (5) ⊦
                debug!("rlineto");
                let p = s.current + v(s.stack[0], s.stack[1]);
                s.path.line_to(p);
                s.stack.clear();
                s.current = p;
                i
            }
            6 => { // ⊦ dx hlineto (6) ⊦
                debug!("hlineto");
                let p = s.current + v(s.stack[0], 0.);
                s.path.line_to(p);
                s.stack.clear();
                s.current = p;
                i
            }
            7 => { // dy vlineto (7)
                debug!("vlineto");
                let p = s.current + v(0., s.stack[0],);
                s.path.line_to(p);
                s.stack.clear();
                s.current = p;
                i
            }
            8 => { // ⊦ dx1 dy1 dx2 dy2 dx3 dy3 rrcurveto (8) ⊦
                debug!("rrcurveto");
                let c1 = s.current + v(s.stack[0], s.stack[1]);
                let c2 = c1 + v(s.stack[2], s.stack[3]);
                let p = c2 + v(s.stack[4], s.stack[5]);
                s.path.bezier_curve_to(c1, c2, p);
                s.stack.clear();
                s.current = p;
                i
            }
            9 => { // –closepath (9) ⊦
                debug!("closepath");
                s.path.close_path();
                s.stack.clear();
                i
            }
            10 => { // subr# callsubr (10) –
                debug!("callsubr");
                let subr_nr = s.pop().to_int();
                let subr = ctx.subr(subr_nr);
                charstring(subr, ctx, s)?;
                i
            }
            11 => { // return
                debug!("return");
                input = i;
                break;
            }
            12 => {
                let (i, b1) = be_u8(i)?;
                match b1 {
                    0 => { // – dotsection (12 0) ⊦
                        debug!("dotsection");
                        s.stack.clear();
                        i
                    }
                    1 => { // ⊦ x0 dx0 x1 dx1 x2 dx2 vstem3 (12 1) ⊦
                        debug!("vstem3");
                        s.stack.clear();
                        i
                    }
                    2 => { // ⊦ y0 dy0 y1 dy1 y2 dy2 hstem3 (12 2) ⊦
                        debug!("hstem3");
                        s.stack.clear();
                        i
                    }
                    6 => { // ⊦ asb adx ady bchar achar seac (12 6) ⊦
                        debug!("seac");
                        s.stack.clear();
                        i
                    }
                    7 => { // ⊦ sbx sby wx wy sbw (12 7) ⊦
                        let (sbx, sby, wx, wy, sbw) = s.args();
                        debug!("sbw");
                        s.char_width = Some(wx.to_float());
                        s.current = v(sbx, sby);
                        s.stack.clear();
                        i
                    }
                    12 => { // num1 num2 div (12 12) quotient
                        debug!("div");
                        let num2 = s.pop().to_float();
                        let num1 = s.pop().to_float();
                        s.push(num1 / num2);
                        i
                    }
                    16 => { //  arg1 . . . argn n othersubr# callothersubr (12 16) –
                        debug!("callothersubr");
                        let subr_nr = s.pop().to_int();
                        i
                    }
                    17 => { // – pop (12 17) number
                        debug!("pop");
                        s.pop();
                        i
                    }
                    33 => { // ⊦ x y sets.currentpoint (12 33) ⊦
                        debug!("sets.currentpoint");
                        let (x, y) = s.args();
                        let p = v(x, y);
                        s.current = p;
                        s.stack.clear();
                        i
                    },
                    _ => panic!("invalid operator")
                }
            }
            13 => { // ⊦ sbx wx hsbw (13) ⊦
                debug!("hsbw");
                let (sbx, wx) = s.args();
                let lsb = v(sbx, 0.);
                s.lsb = Some(lsb);
                s.current = lsb;
                s.char_width = Some(wx.to_float());
                s.stack.clear();
                i
            }
            14 => { //– endchar (14) ⊦
                debug!("endchar");
                input = i;
                break;
            }
            21 => { // ⊦ dx dy rmoveto (21) ⊦
                debug!("rmoveto");
                let (dx, dy) = s.args();
                let p = s.current + v(dx, dy);
                s.path.move_to(p);
                s.current = p;
                s.stack.clear();
                i
            }
            22 => { // ⊦ dx hmoveto (22) ⊦
                debug!("hmoveto");
                let (dx, ) = s.args();
                let p = s.current + v(dx, 0.);
                s.path.move_to(p);
                s.current = p;
                s.stack.clear();
                i
            }
            30 => { // ⊦ dy1 dx2 dy2 dx3 vhcurveto (30) ⊦
                debug!("vhcurveto");
                let (dy1, dx2, dy2, dx3) = s.args();
                let c1 = s.current + v(0., dy1);
                let c2 = c1 + v(dx2, dy2);
                let p = c2 + v(dx3, 0.);
                s.path.bezier_curve_to(c1, c2, p);
                s.stack.clear();
                s.current = p;
                i
            }
            31 => { // ⊦ dx1 dx2 dy2 dy3 hvcurveto (31) ⊦
                debug!("hvcurveto");
                let (dx1, dx2, dy2, dy3) = s.args();
                let c1 = s.current + v(dx1, 0.);
                let c2 = c1 + v(dx2, dy2);
                let p = c2 + v(0., dy3);
                s.path.bezier_curve_to(c1, c2, p);
                s.stack.clear();
                s.current = p;
                i
            },
            v @ 32 ..= 246 => {
                s.push(v as i32 - 139);
                i
            }
            v @ 247 ..= 250 => {
                let (i, w) = be_u8(i)?;
                s.push((v as i32 - 247) * 256 + w as i32 + 108);
                i
            }
            v @ 251 ..= 254 => {
                let (i, w) = be_u8(i)?;
                s.push(-(v as i32 - 251) * 256 - w as i32 - 108);
                i
            }
            255 => {
                let (i, v) = be_i32(i)?;
                s.push(v as f32 / 65536.);
                i
            }
            c => panic!("unknown code {}", c)
        };
        
        input = i;
    };
    
    Ok((input, ()))
}
