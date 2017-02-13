use reader::parser::Parser;
use reader::lexer::Lexer;

use xref::*;
use err::*;
use content::*;
use std::mem::swap;

impl Parser {

    fn content_stream(data: &[u8]) -> Result<()> {
        let mut content = Content {operations: Vec::new()};
        let mut lexer = Lexer::new(data);
        let mut buffer = Vec::new();
        loop {
            let backup_pos = lexer.get_pos();
            let obj = Parser::object(&mut lexer);
            match obj {
                Ok(obj) => buffer.push(obj),
                Err(_) => {
                    lexer.set_pos(backup_pos);
                    // If it's not an object, treat it as an operator
                    let operator = lexer.next()?.as_string(); // TODO will fail because of ' and "
                    let mut operation = Operation::new(operator, Vec::new());
                    // Give operands to operation and empty buffer.
                    swap(&mut buffer, &mut operation.operands);

                }
            }
        }
        Ok(())
    }
}
