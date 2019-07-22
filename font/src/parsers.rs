use nom::{
    bytes::complete::{take_till, tag},
    sequence::{delimited, tuple, preceded},
    combinator::{opt, map, recognize},
    character::complete::{one_of, digit0, digit1, alpha1},
    branch::alt,
};
use crate::R;

fn special_char(b: u8) -> bool {
    match b {
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%' => true,
        _ => false
    }
}

pub fn literal<'a>(i: &'a [u8]) -> R<'a, Vec<u8>> {
    alt((
        map(
            preceded(
                tag("/"),
                take_till(|b| word_sep(b) || special_char(b))
            ),
            |s: &[u8]| s.to_owned()
        ),
        delimited(
            tag("("),
            delimited_literal,
            tag(")")
        )
    ))(i)
}
#[test]
fn test_literal() {
    assert_eq!(
        literal(&b"/FontBBox{-180 -293 1090 1010}readonly def"[..]),
        Ok((&b"{-180 -293 1090 1010}readonly def"[..], b"FontBBox".to_vec()))
    );
}

pub fn integer(i: &[u8]) -> R<i32> {
    map(
        recognize(tuple((
            opt(one_of("+-")),
            digit1
        ))),
        |s| std::str::from_utf8(s).unwrap().parse().unwrap()
    )(i)
}

pub fn plus_minus(i: &[u8]) -> R<&[u8]> {
    alt((tag("+"), tag("-")))(i)
}
pub fn float(i: &[u8]) -> R<f32> {
    map(
        recognize(tuple((
            opt(plus_minus),
            digit0,
            tag("."),
            digit0,
            opt(tuple((
                alt((tag("e"), tag("E"))),
                opt(plus_minus),
                digit1
            ))) 
        ))),
        |s| std::str::from_utf8(s).unwrap().parse::<f32>().expect("overflow")
    )(i)
}
pub fn bound<T>(f: impl Fn(&[u8]) -> R<T>, n: usize) -> impl Fn(&[u8]) -> R<T> {
    move |i: &[u8]| {
        let s = &i[.. i.len().min(n)];
        let map = |r: &[u8]| &i[s.len() - r.len() ..];
        match f(s) {
            Ok((r, t)) => Ok((map(r), t)),
            Err(e) => Err(e)
        }
    }
}
pub fn delimited_literal(i: &[u8]) -> R<Vec<u8>> {
    let mut level = 0;
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(&b) = i.get(pos) {
        match b {
            b')' => {
                if level == 0 {
                    break;
                }
                level -= 1;
                out.push(b);
                pos += 1;
            },
            b'(' => {
                level += 1;
                out.push(b);
                pos += 1;
            }
            b'\\' => {
                if let Some(&c) = i.get(pos+1) {
                    let r = match c {
                        b'n' => b'\n',
                        b'r' => b'\r',
                        b't' => b'\t',
                        b'b' => 8,
                        b'f' => 12,
                        b @ b'\n' | b @ b'\r' => {
                            match (b, i.get(pos+2)) {
                                (b'\n', Some(b'\r')) | (b'\r', Some(b'\n')) => pos += 3,
                                _ => pos += 2,
                            }
                            continue;
                        }
                        c => c
                    };
                    out.push(r);
                    pos += 2;
                } else {
                    break;
                }
            },
            _ => {
                out.push(b);
                pos += 1;
            }
        }
    }
    Ok((&i[pos ..], out))
}

pub fn take_until_and_consume(filter: impl Fn(u8) -> bool) -> impl Fn(&[u8]) -> R<&[u8]> {
    move |i: &[u8]| {
        let end = i.iter()
            .position(|&b| filter(b))
            .unwrap_or(i.len());
            
        let next = end + i[end ..].iter()
            .position(|&b| !filter(b))
            .unwrap_or(i.len());
        
        Ok((&i[next ..], &i[.. end]))
    }
}

pub fn line_sep(b: u8) -> bool {
    match b {
        b'\r' | b'\n' => true,
        _ => false
    }
}
pub fn word_sep(b: u8) -> bool {
    match b {
        b' ' | b'\t' | b'\r' | b'\n' => true,
        _ => false
    }
}

pub fn name(i: &[u8]) -> R<&[u8]> {
    alt((alpha1, tag("["), tag("]")))(i)
}
