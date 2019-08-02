use crate::{State, v, Value, Context};
use nom::{IResult,
    bytes::complete::{take},
    number::complete::{be_u8, be_i16, be_i32}
};

macro_rules! point {
    ($iter:ident, x) => ({
        let &x = $iter.next().unwrap();
        v(x, 0.0)
    });
    ($iter:ident, y) => ({
        let &y = $iter.next().unwrap();
        v(0.0, y)
    });
    ($iter:ident, xy) => ({
        let &x = $iter.next().unwrap();
        let &y = $iter.next().unwrap();
        v(x, y)
    });
    ($iter:ident, yx) => ({
        let &y = $iter.next().unwrap();
        let &x = $iter.next().unwrap();
        v(x, y)
    });
}

macro_rules! bezier {
    ($s:ident, $slice:ident, $($a:tt $b:tt $c:tt)*) => ({
        let mut iter = $slice.iter();
        $(
            let c1 = $s.current + point!(iter, $a);
            let c2 = c1 + point!(iter, $b);
            let p = c2 + point!(iter, $c);
            $s.path.bezier_curve_to(c1, c2, p);
            $s.current = p;
        )*
        iter.as_slice()
    });
}
macro_rules! lines {
    ($s:ident, $slice:ident, $($a:tt)*) => ({
        let mut iter = $slice.iter();
        $(
            let p = $s.current + point!(iter, $a);
            $s.path.line_to(p);
            $s.current = p;
        )*
        iter.as_slice()
    });
}

fn alternating_curve(s: &mut State, mut horizontal: bool) {
    let mut slice = s.stack.as_slice();
    while slice.len() > 0 {
        slice = match (slice.len(), horizontal) {
            (5, false) => bezier!(s, slice, y xy xy),
            (5, true)  => bezier!(s, slice, x xy yx),
            (_, false)  => bezier!(s, slice, y xy x),
            (_, true) => bezier!(s, slice, x xy y),
        };
        horizontal = !horizontal;
    }
}
fn maybe_width(state: &mut State, cond: impl Fn(usize) -> bool) {
    if state.first_stack_clearing_operator {
        state.first_stack_clearing_operator = false;
        if !cond(state.stack.len()) {
            let w = state.stack.remove(0);
            state.delta_width = Some(w.to_float());
        }
    }
}
pub fn charstring<'a, 'b>(mut input: &'a [u8], ctx: &'a Context<'a>, s: &'b mut State) -> IResult<&'a [u8], ()> {
    while input.len() > 0 && !s.done {
        let (i, b0) = be_u8(input)?;
        let i = match b0 {
            0 => panic!("reserved"),
            1 => { // ⊦ y dy hstem (1) ⊦
                debug!("hstem");
                maybe_width(s, |n| n == 2);
                s.stem_hints += (s.stack.len() / 2) as u32;
                s.stack.clear();
                i
            }
            2 => panic!("reserved"),
            3 => { // ⊦ x dx vstem (3) ⊦
                debug!("vstem");
                maybe_width(s, |n| n == 2);
                s.stem_hints += (s.stack.len() / 2) as u32;
                s.stack.clear();
                i
            }
            4 => { // ⊦ dy vmoveto (4) ⊦
                debug!("vmoveto");
                maybe_width(s, |n| n == 1);
                let p = s.current + v(0., s.stack[0]);
                s.path.move_to(p);
                s.stack.clear();
                s.current = p;
                i
            }
            5 => { // |- {dxa dya}+ rlineto (5) |-
                debug!("rlineto");
                let mut slice = s.stack.as_slice();
                while slice.len() >= 2 {
                    slice = lines!(s, slice, xy);
                }
                s.stack.clear();
                i
            }
            6 => { // |- dx1 {dya dxb}* hlineto (6) |-
                   // |- {dxa dyb}+ hlineto (6) |-
                debug!("hlineto");
                for (i, &d) in s.stack.iter().enumerate() {
                    let dv = if i % 2 == 0 {
                        v(d, 0.)
                    } else {
                        v(0., d)
                    };
                    let p = s.current + dv;
                    s.path.line_to(p);
                    s.current = p;
                }
                s.stack.clear();
                i
            }
            7 => { // |- dy1 {dxa dyb}* vlineto (7) |-
                   // |- {dya dxb}+ vlineto (7) |-
                debug!("vlineto");
                for (i, &d) in s.stack.iter().enumerate() {
                    let dv = if i % 2 == 0 {
                        v(0., d)
                    } else {
                        v(d, 0.)
                    };
                    let p = s.current + dv;
                    s.path.line_to(p);
                    s.current = p;
                }
                s.stack.clear();
                i
            }
            8 => { // ⊦ {dxa dya dxb dyb dxc dyc}+ rrcurveto (8) ⊦
                debug!("rrcurveto");
                let mut slice = s.stack.as_slice();
                while slice.len() >= 6 {
                    slice = bezier!(s, slice, xy xy xy);
                }
                s.stack.clear();
                i
            }
            9 => panic!("reserved"),
            10 => { // subr# callsubr (10) –
                debug!("callsubr");
                let subr_nr = s.pop().to_int();
                
                let subr = ctx.subr(subr_nr);
                let (_, _) = charstring(subr, ctx, s)?;
                i
            }
            11 => { // – return (11) –
                debug!("return");
                return Ok((i, ()));
            }
            12 => {
                let (i, b1) = be_u8(i)?;
                match b1 {
                    0 | 1 | 2 => panic!("reserved"),
                    3 => unimplemented!("and"),
                    4 => unimplemented!("or"),
                    5 => unimplemented!("not"),
                    6 | 7 | 8 => panic!("reserved"),
                    9 => { // num abs (12 9) num2
                        debug!("abs");
                        match s.pop() {
                            Value::Int(i) => s.push(i.abs()),
                            Value::Float(f) => s.push(f.abs())
                        }
                        i
                    }
                    10 => { // num1 num2 add (12 10) sum
                        debug!("add");
                        match (s.pop(), s.pop()) {
                            (Value::Int(num2), Value::Int(num1)) => s.push(num1 + num2),
                            (num2, num1) => s.push(num2.to_float() + num1.to_float())
                        }
                        i
                    }
                    11 => { // num1 num2 sub (12 11) difference
                        debug!("sub");
                        match (s.pop(), s.pop()) {
                            (Value::Int(num2), Value::Int(num1)) => s.push(num1 - num2),
                            (num2, num1) => s.push(num2.to_float() - num1.to_float())
                        }
                        i
                    }
                    12 => { // num1 num2 div (12 12) quotient
                        debug!("div");
                        let num2 = s.pop().to_float();
                        let num1 = s.pop().to_float();
                        s.push(num1 / num2);
                        i
                    }
                    13 => panic!("reserved"),
                    14 => { // num neg (12 14) num2
                        debug!("neg");
                        match s.pop() {
                            Value::Int(i) => s.push(-i),
                            Value::Float(f) => s.push(-f)
                        }
                        i
                    }
                    15 => unimplemented!("eq"),
                    16 | 17 => panic!("reserved"),
                    18 => { // num drop (12 18)
                        debug!("drop");
                        s.pop();
                        i
                    }
                    19 => panic!("reserved"),
                    20 => unimplemented!("put"),
                    21 => unimplemented!("get"),
                    22 => unimplemented!("ifelse"),
                    23 => { // random (12 23) num2
                        debug!("random");
                        use rand::{thread_rng, Rng};
                        use rand::distributions::OpenClosed01;
                        
                        let val: f32 = thread_rng().sample(OpenClosed01);
                        s.push(val);
                        i
                    }
                    24 => { // num1 num2 mul (12 24) product
                        debug!("mul");
                        let num2 = s.pop().to_float();
                        let num1 = s.pop().to_float();
                        s.push(num1 * num2);
                        i
                    }
                    25 => panic!("reserved"),
                    26 => { // num sqrt (12 26) num2
                        debug!("sqrt");
                        let num1 = s.pop().to_float();
                        s.push(num1.sqrt());
                        i
                    }
                    27 => { // any dup (12 27) any any
                        debug!("dup");
                        let any = s.pop();
                        s.push(any);
                        s.push(any);
                        i
                    }
                    28 => { // num1 num2 exch (12 28) num2 num1
                        debug!("exch");
                        let num2 = s.pop();
                        let num1 = s.pop();
                        s.push(num2);
                        s.push(num1);
                        i
                    }
                    29 => { // numX ... num0 i index (12 29) numX ... num0 numi
                        debug!("index");
                        let j = s.pop().to_int().max(0) as usize;
                        let idx = s.stack.len() - j - 1;
                        let val = s.stack[idx];
                        s.push(val);
                        i
                    }
                    30 => { // num(N–1) ... num0 N J roll (12 30) num((J–1) mod N) ... num0 num(N–1) ... num(J mod N)
                        debug!("roll");
                        let j = s.pop().to_int();
                        let n = s.pop().to_uint() as usize;
                        let len = s.stack.len();
                        let slice = &mut s.stack[len - n - 1 .. len - 1];
                        if j > 0 {
                            slice.rotate_left(j as usize);
                        } else if j < 0 {
                            slice.rotate_right((-j) as usize);
                        }
                        i
                    }
                    31 | 32 | 33 => panic!("reserved"),
                    34 => { // |- dx1 dx2 dy2 dx3 dx4 dx5 dx6 hflex (12 34) |-
                        debug!("hflex");
                        let slice = s.stack.as_slice();
                        bezier!(s, slice, x xy x  x x x);
                        s.stack.clear();
                        i
                    }
                    35 => { // |- dx1 dy1 dx2 dy2 dx3 dy3 dx4 dy4 dx5 dy5 dx6 dy6 fd flex (12 35) |-
                        debug!("flex");
                        let slice = s.stack.as_slice();
                        bezier!(s, slice, xy xy xy  xy xy xy);
                        s.stack.clear();
                        i
                    }
                    36 => { // |- dx1 dy1 dx2 dy2 dx3 dx4 dx5 dy5 dx6 hflex1 (12 36) |-
                        debug!("hflex1");
                        let slice = s.stack.as_slice();
                        bezier!(s, slice, xy xy x  x xy x);
                        s.stack.clear();
                        i
                    }
                    37 => { // |- dx1 dy1 dx2 dy2 dx3 dy3 dx4 dy4 dx5 dy5 d6 flex1 (12 37) |-
                        debug!("flex1");
                        let slice = s.stack.as_slice();
                        
                        // process first bezier
                        bezier!(s, slice, xy xy xy);
                        
                        // figure out the second
                        let mut iter = slice.iter();
                        let mut sum = point!(iter, xy);
                        for _ in 0 ..  4 {
                            sum = sum + point!(iter, xy);
                        }
                        let horizontal = sum.x().abs() > sum.y().abs();
                        
                        let mut iter = slice[6..].iter();
                        let d4 = s.current + point!(iter, xy);
                        let d5 = d4 + point!(iter, xy);
                        let d6 = d5 + match horizontal {
                            true => point!(iter, x),
                            false => point!(iter, y)
                        };
                        s.path.bezier_curve_to(d4, d5, d6);
                        s.current = d6;
                        s.stack.clear();
                        i
                    }
                    38 ..= 255 => panic!("reserved")
                }
            }
            13 => panic!("reserved"),
            14 => { //– endchar (14) ⊦
                debug!("endchar");
                maybe_width(s, |n| n == 0);
                s.path.close_path();
                s.done = true;
                i
            }
            15 | 16 | 17 => panic!("reserved"),
            18 => { // |- y dy {dya dyb}* hstemhm (18) |-
                debug!("hstemhm");
                maybe_width(s, |n| n % 2 == 0);
                s.stem_hints += (s.stack.len() / 2) as u32;
                s.stack.clear();
                i
            }
            19 => { // |- hintmask (19 + mask) |-
                debug!("hintmask");
                maybe_width(s, |n| n == 0);
                s.stem_hints += (s.stack.len() / 2) as u32;
                let (i, _) = take((s.stem_hints + 7) / 8)(i)?;
                s.stack.clear();
                i
            }
            20 => { // cntrmask |- cntrmask (20 + mask) |-
                debug!("cntrmask");
                maybe_width(s, |n| n == 0);
                s.stem_hints += (s.stack.len() / 2) as u32;
                let (i, _) = take((s.stem_hints + 7) / 8)(i)?;
                s.stack.clear();
                i
            }
            21 => { // ⊦ dx dy rmoveto (21) ⊦
                debug!("rmoveto");
                maybe_width(s, |n| n == 2);
                let p = s.current + v(s.stack[0], s.stack[1]);
                s.path.move_to(p);
                s.current = p;
                s.stack.clear();
                i
            }
            22 => { // ⊦ dx hmoveto (22) ⊦
                debug!("hmoveto");
                maybe_width(s, |n| n == 1);
                let p = s.current + v(s.stack[0], 0.);
                s.path.move_to(p);
                s.current = p;
                s.stack.clear();
                i
            }
            23 => { // |- x dx {dxa dyx}* vstemhm (23) |-
                debug!("vstemhm");
                maybe_width(s, |n| n % 2 == 0);
                s.stem_hints += (s.stack.len() / 2) as u32;
                s.stack.clear();
                i
            }
            24 => { // |- {dxa dya dxb dyb dxc dyc}+ dxd dyd rcurveline (24) |-
                debug!("rcurveline");
                let mut slice = s.stack.as_slice();
                while slice.len() >= 8 {
                    slice = bezier!(s, slice, xy xy xy);
                }
                lines!(s, slice, xy);
                
                s.stack.clear();
                i
            }
            25 => { // |- {dxa dya}+ dxb dyb dxc dyc dxd dyd rlinecurve (25) |-
                debug!("rlinecurve");
                let mut slice = s.stack.as_slice();
                while slice.len() >= 8 {
                    slice = lines!(s, slice, xy);
                }
                bezier!(s, slice, xy xy xy);
                
                s.stack.clear();
                i
            }
            26 => { // |- dx1? {dya dxb dyb dyc}+ vvcurveto (26) |-
                debug!("vvcurveto");
                let mut slice = s.stack.as_slice();
                if slice.len() % 2 == 1 { // odd 
                    slice = bezier!(s, slice, xy xy y);
                }
                while slice.len() >= 4 {
                    slice = bezier!(s, slice, y xy y);
                }
                s.stack.clear();
                i
            }
            27 => { // ⊦ dy1? {dxa dxb dyb dxc}+ hhcurveto (27) ⊦
                debug!("hhcurveto");
                let mut slice = s.stack.as_slice();
                if slice.len() % 2 == 1 { // odd 
                    slice = bezier!(s, slice, yx xy x);
                }
                while slice.len() >= 4 {
                    slice = bezier!(s, slice, x xy x);
                }
                s.stack.clear();
                i
            }
            29 => { // globalsubr# callgsubr (29) –
                let subr_nr = s.pop().to_int();
                debug!("globalsubr#{}", subr_nr as i32 + ctx.global_subr_bias);
                
                let subr = ctx.global_subr(subr_nr);
                let (_, _) = charstring(subr, ctx, s)?;
                i
            }
            30 => { // |- dy1 dx2 dy2 dx3 {dxa dxb dyb dyc dyd dxe dye dxf}* dyf? vhcurveto (30) |-
                    // |- {dya dxb dyb dxc dxd dxe dye dyf}+ dxf? vhcurveto (30) |-
                debug!("vhcurveto");
                alternating_curve(s, false);
                
                s.stack.clear();
                i
            }
            31 => { // |- dx1 dx2 dy2 dy3 {dya dxb dyb dxc dxd dxe dye dyf}* dxf? hvcurveto (31) |-
                    // |- {dxa dxb dyb dyc dyd dxe dye dxf}+ dyf? hvcurveto (31) |-
                debug!("hvcurveto");
                alternating_curve(s, true);
                
                s.stack.clear();
                i
            },
            28 => {
                let (i, v) = be_i16(i)?;
                s.push(v);
                i
            }
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
        };
        
        input = i;
    };
    
    Ok((input, ()))
}
