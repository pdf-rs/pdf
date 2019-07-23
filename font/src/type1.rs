use std::io::{self, Read};
use std::error::Error;
use nom::{IResult,
    number::complete::{be_u8, le_u8, be_i32, le_u32},
};
use crate::{Font, Glyph, Context, State, v, R, IResultExt};
use crate::postscript::{Vm};
use crate::eexec::Decoder;

pub struct Type1Font {
}
impl Font for Type1Font {
    fn num_glyphs(&self) -> u32 { 0 }
    fn glyph(&self, _id: u32) -> Result<Glyph, Box<dyn Error>> {
        unimplemented!()
    }
}
impl Type1Font {
    pub fn parse_pfb(data: &[u8]) -> Result<Self, Box<dyn Error>> {
        Ok(parse_pfb(data).get())
    }
}

fn parse_binary<'a>(vm: &mut Vm, data: &'a [u8]) {
    let mut decoder = Decoder::file();
    let decoded = decoder.decode(data);
    
    vm.parse_and_exec(&decoded[4 ..])
}

#[test]
fn test_parser() {
    let mut vm = Vm::new();
    parse_text(&mut vm, b"/FontBBox{-180 -293 1090 1010}readonly ");
    vm.print_stack();
    assert_eq!(vm.stack().len(), 2);
}
fn parse_pfb(i: &[u8]) -> R<Type1Font> {
    let mut vm = Vm::new();
    
    let mut input = i;
    while input.len() > 0 {
    let (i, magic) = le_u8(input)?;
        assert_eq!(magic, 0x80);
        let (i, block_type) = le_u8(i)?;
        
        let (i, block_len) = le_u32(i)?;
        info!("block type {}, length: {}", block_type, block_len);
    
        let block = &i[.. block_len as usize];
        match block_type {
            1 => vm.parse_and_exec(block),
            2 => parse_binary(&mut vm, block),
            n => panic!("unknown block type {}", n)
        }
        
        input = &i[block_len as usize ..];
    }
    
    panic!()
}
pub fn charstring<'a, 'b>(mut input: &'a [u8], ctx: &Context<'a>, s: &'b mut State) -> IResult<&'a [u8], ()> {
    let i = loop {
        debug!("stack: {:?}", s.stack);
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
                let subr = ctx.private_subroutine(subr_nr);
                let (i, _) = charstring(subr, ctx, s)?;
                i
            }
            14 => { //– endchar (14) ⊦
                debug!("endchar");
                break i;
            }
            13 => { // ⊦ sbx wx hsbw (13) ⊦
                debug!("hsbw");
                s.lsp = Some(v(s.stack[0], 0.));
                s.char_width = Some(s.stack[1].into());
                s.stack.clear();
                i
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
                        debug!("sbw");
                        s.stack.clear();
                        i
                    }
                    11 => { // – return (11) –
                        debug!("return");
                        break i;
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
                        unimplemented!()
                    }
                    17 => { // – pop (12 17) number
                        debug!("pop");
                        unimplemented!()
                    }
                    33 => { // ⊦ x y sets.currentpoint (12 33) ⊦
                        debug!("sets.currentpoint");
                        let p = v(s.stack[0], s.stack[1]);
                        s.current = p;
                        s.stack.clear();
                        i
                    },
                    _ => panic!("invalid operator")
                }
            }
            21 => { // ⊦ dx dy rmoveto (21) ⊦
                debug!("rmoveto");
                let p = s.current + v(s.stack[0], s.stack[1]);
                s.path.move_to(p);
                s.current = p;
                s.stack.clear();
                i
            }
            22 => { // ⊦ dx hmoveto (22) ⊦
                debug!("hmoveto");
                let p = s.current + v(s.stack[0], 0.);
                s.path.move_to(p);
                s.current = p;
                s.stack.clear();
                i
            }
            30 => { // ⊦ dy1 dx2 dy2 dx3 vhcurveto (30) ⊦
                debug!("vhcurveto");
                let c1 = s.current + v(0., s.stack[0]);
                let c2 = c1 + v(s.stack[1], s.stack[2]);
                let p = c2 + v(s.stack[3], 0.);
                s.path.bezier_curve_to(c1, c2, p);
                s.stack.clear();
                s.current = p;
                i
            }
            31 => { // ⊦ dx1 dx2 dy2 dy3 hvcurveto (31) ⊦
                debug!("hvcurveto");
                let c1 = s.current + v(s.stack[0], s.stack[1]);
                let c2 = c1 + v(0., s.stack[2]);
                let p = c2 + v(0., s.stack[3]);
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
    
    Ok((i, ()))
}
