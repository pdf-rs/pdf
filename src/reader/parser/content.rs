use reader::parser::Parser;
use reader::lexer::Lexer;

use err::*;
use content::*;
use std::mem::swap;

impl Parser {

    pub fn content_stream(data: &[u8]) -> Result<Content> {
        let mut lexer = Lexer::new(data);

        let mut content = Content {operations: Vec::new()};
        let mut buffer = Vec::new();

        loop {
            let backup_pos = lexer.get_pos();
            let obj = Parser::object(&mut lexer);
            match obj {
                Ok(obj) => {
                    // Operand
                    buffer.push(obj)
                }
                Err(e) => {
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
