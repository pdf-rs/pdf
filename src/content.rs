/// PDF content streams.
use file::Reader;
use file::Object;

use std;
use std::fmt::{Display, Formatter};
use err::*;
use std::mem::swap;
use file::lexer::Lexer;

/// Operation in a PDF content stream.
#[derive(Debug, Clone)]
pub struct Operation {
	pub operator: String,
	pub operands: Vec<Object>,
}

impl Operation {
	pub fn new(operator: String, operands: Vec<Object>) -> Operation {
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
    pub fn parse_from(data: &[u8]) -> Result<Content> {
        let mut lexer = Lexer::new(data);

        let mut content = Content {operations: Vec::new()};
        let mut buffer = Vec::new();

        loop {
            let backup_pos = lexer.get_pos();
            let obj = Reader::parse_object_as_is(&mut lexer);
            match obj {
                Ok(obj) => {
                    // Operand
                    buffer.push(obj)
                }
                Err(_) => {
                    // It's not an object/operand - treat it as an operator.
                    lexer.set_pos(backup_pos);
                    let operator = lexer.next()?.as_string(); // TODO will this work as expected?
                    let mut operation = Operation::new(operator, Vec::new());
                    // Give operands to operation and empty buffer.
                    swap(&mut buffer, &mut operation.operands);
                    content.operations.push(operation.clone());
                }
            }
            if lexer.get_pos() > data.len() {
                bail!("Read past boundary of given contents.");
            } else if lexer.get_pos() == data.len() {
                break;
            }
        }
        Ok(content)
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
            write!(f, "{}, ", operand)?;
        }
        write!(f, ")\n")
    }
}
