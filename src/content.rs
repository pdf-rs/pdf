use object::*;
use std::fmt::{Display, Formatter};
use std;

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

pub struct Content {
    pub operations: Vec<Operation>,
}


impl Display for Content {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "Content: ")?;
        for ref operation in &self.operations {
            write!(f, "{}", operation)?;
        }
        Ok(())
    }
}

impl Display for Operation {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "Operation: {} (", self.operator)?;
        for ref operand in &self.operands {
            write!(f, "{}, ", operand)?;
        }
        write!(f, ") ,  ")
    }
}
