/// PDF content streams.
use std;
use std::fmt::{Display, Formatter};
use std::mem::swap;
use std::io;

use err::*;
use object::*;
use parser::{Lexer, parse_with_lexer};
use primitive::*;
use object::decode_fully;

/// Operation in a PDF content stream.
#[derive(Debug, Clone)]
pub struct Operation {
	pub operator: String,
	pub operands: Vec<Primitive>,
}

impl Operation {
	pub fn new(operator: String, operands: Vec<Primitive>) -> Operation {
		Operation{
			operator: operator,
			operands: operands,
		}
	}
}


/// Represents a PDF content stream - a `Vec` of `Operator`s
#[derive(Debug)]
pub struct Content {
    pub operations: Vec<Operation>,
}

impl Content {
    fn parse_from(data: &[u8], resolve: &Resolve) -> Result<Content> {
        let mut lexer = Lexer::new(data);

        let mut content = Content {operations: Vec::new()};
        let mut buffer = Vec::new();

        loop {
            let backup_pos = lexer.get_pos();
            let obj = parse_with_lexer(&mut lexer, resolve);
            match obj {
                Ok(obj) => {
                    // Operand
                    buffer.push(obj)
                }
                Err(_) => {
                    // It's not an object/operand - treat it as an operator.
                    lexer.set_pos(backup_pos);
                    let operator = lexer.next()?.to_string();
                    let mut operation = Operation::new(operator, Vec::new());
                    // Give operands to operation and empty buffer.
                    swap(&mut buffer, &mut operation.operands);
                    content.operations.push(operation.clone());
                }
            }
            if lexer.get_pos() > data.len() {
                bail!(ErrorKind::ContentReadPastBoundary);
            } else if lexer.get_pos() == data.len() {
                break;
            }
        }
        Ok(content)
    }
}

impl Object for Content {
    /// Write object as a byte stream
    fn serialize<W: io::Write>(&self, _out: &mut W) -> io::Result<()> {unimplemented!()}
    /// Convert primitive to Self
    fn from_primitive(p: Primitive, resolve: &Resolve) -> Result<Self> {
        let PdfStream {info, mut data} = PdfStream::from_primitive(p, resolve)?;
        let mut info = StreamInfo::<()>::from_primitive(Primitive::Dictionary (info), resolve)?;
        decode_fully(&mut data, &mut info.filters)?;
        Content::parse_from(&data, resolve)
    }
}


impl Display for Content {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "Content: ")?;
        for operation in &self.operations {
            write!(f, "{}", operation)?;
        }
        Ok(())
    }
}

impl Display for Operation {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "Operation: {} (", self.operator)?;
        for operand in &self.operands {
            write!(f, "{:?}, ", operand)?;
        }
        write!(f, ")\n")
    }
}
