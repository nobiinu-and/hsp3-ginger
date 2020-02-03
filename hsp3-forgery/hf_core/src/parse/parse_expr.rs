use super::*;

type Px = ParseContext;

pub(crate) fn parse_expr(p: &mut Px) {
    match p.next() {
        Token::Ident => parse_name(p),
        Token::Star => parse_label_literal(p),
        _ => {
            // unimplemented
            p.bump();
        }
    }
}
