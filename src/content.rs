use object::*;

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
