//! Statement and declaration parsing.
//!
//! More information:
//!  - [MDN documentation][mdn]
//!  - [ECMAScript specification][spec]
//!
//! [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Statements
//! [spec]: https://tc39.es/ecma262/#sec-ecmascript-language-statements-and-declarations

mod block;
mod break_stm;
mod continue_stm;
mod declaration;
mod expression;
mod if_stm;
mod iteration;
mod labelled_stm;
mod return_stm;
mod switch;
mod throw;
mod try_stm;
mod variable;

use self::{
    block::BlockStatement,
    break_stm::BreakStatement,
    continue_stm::ContinueStatement,
    declaration::Declaration,
    expression::ExpressionStatement,
    if_stm::IfStatement,
    iteration::{DoWhileStatement, ForStatement, WhileStatement},
    labelled_stm::LabelledStatement,
    return_stm::ReturnStatement,
    switch::SwitchStatement,
    throw::ThrowStatement,
    try_stm::TryStatement,
    variable::VariableStatement,
};
use super::{
    expression::PropertyName, AllowAwait, AllowIn, AllowReturn, AllowYield, Cursor, ParseError,
    TokenParser,
};
use crate::syntax::{
    ast::{
        node::{
            self,
            declaration::{
                BindingPatternTypeArray, BindingPatternTypeObject, DeclarationPattern,
                DeclarationPatternArray, DeclarationPatternObject,
            },
        },
        Keyword, Node, Punctuator,
    },
    lexer::{Error as LexError, InputElement, Token, TokenKind},
    parser::expression::{await_expr::AwaitExpression, Initializer},
};
use boa_interner::{Interner, Sym};
use boa_profiler::Profiler;
use std::{io::Read, vec};

pub(in crate::syntax::parser) use declaration::ClassTail;
pub(in crate::syntax) use declaration::PrivateElement;

/// Statement parsing.
///
/// This can be one of the following:
///
///  - `BlockStatement`
///  - `VariableStatement`
///  - `EmptyStatement`
///  - `ExpressionStatement`
///  - `IfStatement`
///  - `BreakableStatement`
///  - `ContinueStatement`
///  - `BreakStatement`
///  - `ReturnStatement`
///  - `WithStatement`
///  - `LabelledStatement`
///  - `ThrowStatement`
///  - `SwitchStatement`
///  - `TryStatement`
///  - `DebuggerStatement`
///
/// More information:
///  - [MDN documentation][mdn]
///  - [ECMAScript specification][spec]
///
/// [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Statements
/// [spec]: https://tc39.es/ecma262/#prod-Statement
#[derive(Debug, Clone, Copy)]
pub(super) struct Statement {
    allow_yield: AllowYield,
    allow_await: AllowAwait,
    allow_return: AllowReturn,
}

impl Statement {
    /// Creates a new `Statement` parser.
    pub(super) fn new<Y, A, R>(allow_yield: Y, allow_await: A, allow_return: R) -> Self
    where
        Y: Into<AllowYield>,
        A: Into<AllowAwait>,
        R: Into<AllowReturn>,
    {
        Self {
            allow_yield: allow_yield.into(),
            allow_await: allow_await.into(),
            allow_return: allow_return.into(),
        }
    }
}

impl<R> TokenParser<R> for Statement
where
    R: Read,
{
    type Output = Node;

    fn parse(
        self,
        cursor: &mut Cursor<R>,
        interner: &mut Interner,
    ) -> Result<Self::Output, ParseError> {
        let _timer = Profiler::global().start_event("Statement", "Parsing");
        // TODO: add BreakableStatement and divide Whiles, fors and so on to another place.
        let tok = cursor.peek(0, interner)?.ok_or(ParseError::AbruptEnd)?;

        match tok.kind() {
            TokenKind::Keyword((Keyword::Await, _)) => AwaitExpression::new(self.allow_yield)
                .parse(cursor, interner)
                .map(Node::from),
            TokenKind::Keyword((Keyword::If, _)) => {
                IfStatement::new(self.allow_yield, self.allow_await, self.allow_return)
                    .parse(cursor, interner)
                    .map(Node::from)
            }
            TokenKind::Keyword((Keyword::Var, _)) => {
                VariableStatement::new(self.allow_yield, self.allow_await)
                    .parse(cursor, interner)
                    .map(Node::from)
            }
            TokenKind::Keyword((Keyword::While, _)) => {
                WhileStatement::new(self.allow_yield, self.allow_await, self.allow_return)
                    .parse(cursor, interner)
                    .map(Node::from)
            }
            TokenKind::Keyword((Keyword::Do, _)) => {
                DoWhileStatement::new(self.allow_yield, self.allow_await, self.allow_return)
                    .parse(cursor, interner)
                    .map(Node::from)
            }
            TokenKind::Keyword((Keyword::For, _)) => {
                ForStatement::new(self.allow_yield, self.allow_await, self.allow_return)
                    .parse(cursor, interner)
                    .map(Node::from)
            }
            TokenKind::Keyword((Keyword::Return, _)) => {
                if self.allow_return.0 {
                    ReturnStatement::new(self.allow_yield, self.allow_await)
                        .parse(cursor, interner)
                        .map(Node::from)
                } else {
                    Err(ParseError::unexpected(
                        tok.to_string(interner),
                        tok.span(),
                        "statement",
                    ))
                }
            }
            TokenKind::Keyword((Keyword::Break, _)) => {
                BreakStatement::new(self.allow_yield, self.allow_await)
                    .parse(cursor, interner)
                    .map(Node::from)
            }
            TokenKind::Keyword((Keyword::Continue, _)) => {
                ContinueStatement::new(self.allow_yield, self.allow_await)
                    .parse(cursor, interner)
                    .map(Node::from)
            }
            TokenKind::Keyword((Keyword::Try, _)) => {
                TryStatement::new(self.allow_yield, self.allow_await, self.allow_return)
                    .parse(cursor, interner)
                    .map(Node::from)
            }
            TokenKind::Keyword((Keyword::Throw, _)) => {
                ThrowStatement::new(self.allow_yield, self.allow_await)
                    .parse(cursor, interner)
                    .map(Node::from)
            }
            TokenKind::Keyword((Keyword::Switch, _)) => {
                SwitchStatement::new(self.allow_yield, self.allow_await, self.allow_return)
                    .parse(cursor, interner)
                    .map(Node::from)
            }
            TokenKind::Punctuator(Punctuator::OpenBlock) => {
                BlockStatement::new(self.allow_yield, self.allow_await, self.allow_return)
                    .parse(cursor, interner)
                    .map(Node::from)
            }
            TokenKind::Punctuator(Punctuator::Semicolon) => {
                // parse the EmptyStatement
                cursor.next(interner).expect("semicolon disappeared");
                Ok(Node::Empty)
            }
            TokenKind::Identifier(_) => {
                // Labelled Statement check
                cursor.set_goal(InputElement::Div);
                let tok = cursor.peek(1, interner)?;

                if let Some(tok) = tok {
                    if matches!(tok.kind(), TokenKind::Punctuator(Punctuator::Colon)) {
                        return LabelledStatement::new(
                            self.allow_yield,
                            self.allow_await,
                            self.allow_return,
                        )
                        .parse(cursor, interner)
                        .map(Node::from);
                    }
                }

                ExpressionStatement::new(self.allow_yield, self.allow_await).parse(cursor, interner)
            }

            _ => {
                ExpressionStatement::new(self.allow_yield, self.allow_await).parse(cursor, interner)
            }
        }
    }
}

/// Reads a list of statements.
///
/// More information:
///  - [ECMAScript specification][spec]
///
/// [spec]: https://tc39.es/ecma262/#prod-StatementList
#[derive(Debug, Clone, Copy)]
pub(super) struct StatementList {
    allow_yield: AllowYield,
    allow_await: AllowAwait,
    allow_return: AllowReturn,
    in_block: bool,
    break_nodes: &'static [TokenKind],
}

impl StatementList {
    /// Creates a new `StatementList` parser.
    pub(super) fn new<Y, A, R>(
        allow_yield: Y,
        allow_await: A,
        allow_return: R,
        in_block: bool,
        break_nodes: &'static [TokenKind],
    ) -> Self
    where
        Y: Into<AllowYield>,
        A: Into<AllowAwait>,
        R: Into<AllowReturn>,
    {
        Self {
            allow_yield: allow_yield.into(),
            allow_await: allow_await.into(),
            allow_return: allow_return.into(),
            in_block,
            break_nodes,
        }
    }
}

impl<R> TokenParser<R> for StatementList
where
    R: Read,
{
    type Output = node::StatementList;

    /// The function parses a `node::StatementList` using the `StatementList`'s
    /// `break_nodes` to know when to terminate.
    ///
    /// Returns a `ParseError::AbruptEnd` if end of stream is reached before a
    /// break token.
    ///
    /// Returns a `ParseError::unexpected` if an unexpected token is found.
    ///
    /// Note that the last token which causes the parse to finish is not
    /// consumed.
    fn parse(
        self,
        cursor: &mut Cursor<R>,
        interner: &mut Interner,
    ) -> Result<Self::Output, ParseError> {
        let _timer = Profiler::global().start_event("StatementList", "Parsing");
        let mut items = Vec::new();

        loop {
            match cursor.peek(0, interner)? {
                Some(token) if self.break_nodes.contains(token.kind()) => break,
                None => break,
                _ => {}
            }

            let item = StatementListItem::new(
                self.allow_yield,
                self.allow_await,
                self.allow_return,
                self.in_block,
            )
            .parse(cursor, interner)?;
            items.push(item);

            // move the cursor forward for any consecutive semicolon.
            while cursor.next_if(Punctuator::Semicolon, interner)?.is_some() {}
        }

        items.sort_by(Node::hoistable_order);

        Ok(items.into())
    }
}

/// Statement list item parsing
///
/// A statement list item can either be an statement or a declaration.
///
/// More information:
///  - [MDN documentation][mdn]
///  - [ECMAScript specification][spec]
///
/// [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Statements
/// [spec]: https://tc39.es/ecma262/#prod-StatementListItem
#[derive(Debug, Clone, Copy)]
struct StatementListItem {
    allow_yield: AllowYield,
    allow_await: AllowAwait,
    allow_return: AllowReturn,
    in_block: bool,
}

impl StatementListItem {
    /// Creates a new `StatementListItem` parser.
    fn new<Y, A, R>(allow_yield: Y, allow_await: A, allow_return: R, in_block: bool) -> Self
    where
        Y: Into<AllowYield>,
        A: Into<AllowAwait>,
        R: Into<AllowReturn>,
    {
        Self {
            allow_yield: allow_yield.into(),
            allow_await: allow_await.into(),
            allow_return: allow_return.into(),
            in_block,
        }
    }
}

impl<R> TokenParser<R> for StatementListItem
where
    R: Read,
{
    type Output = Node;

    fn parse(
        self,
        cursor: &mut Cursor<R>,
        interner: &mut Interner,
    ) -> Result<Self::Output, ParseError> {
        let _timer = Profiler::global().start_event("StatementListItem", "Parsing");
        let strict_mode = cursor.strict_mode();
        let tok = cursor.peek(0, interner)?.ok_or(ParseError::AbruptEnd)?;

        match *tok.kind() {
            TokenKind::Keyword((Keyword::Function | Keyword::Async | Keyword::Class, _)) => {
                if strict_mode && self.in_block {
                    return Err(ParseError::lex(LexError::Syntax(
                        "Function declaration in blocks not allowed in strict mode".into(),
                        tok.span().start(),
                    )));
                }
                Declaration::new(self.allow_yield, self.allow_await, true).parse(cursor, interner)
            }
            TokenKind::Keyword((Keyword::Const | Keyword::Let, _)) => {
                Declaration::new(self.allow_yield, self.allow_await, true).parse(cursor, interner)
            }
            _ => Statement::new(self.allow_yield, self.allow_await, self.allow_return)
                .parse(cursor, interner),
        }
    }
}

/// Label identifier parsing.
///
/// This seems to be the same as a `BindingIdentifier`.
///
/// More information:
///  - [ECMAScript specification][spec]
///
/// [spec]: https://tc39.es/ecma262/#prod-LabelIdentifier
pub(super) type LabelIdentifier = BindingIdentifier;

/// Binding identifier parsing.
///
/// More information:
///  - [ECMAScript specification][spec]
///
/// [spec]: https://tc39.es/ecma262/#prod-BindingIdentifier
#[derive(Debug, Clone, Copy)]
pub(super) struct BindingIdentifier {
    allow_yield: AllowYield,
    allow_await: AllowAwait,
}

impl BindingIdentifier {
    /// Creates a new `BindingIdentifier` parser.
    pub(super) fn new<Y, A>(allow_yield: Y, allow_await: A) -> Self
    where
        Y: Into<AllowYield>,
        A: Into<AllowAwait>,
    {
        Self {
            allow_yield: allow_yield.into(),
            allow_await: allow_await.into(),
        }
    }
}

impl<R> TokenParser<R> for BindingIdentifier
where
    R: Read,
{
    type Output = Sym;

    /// Strict mode parsing as per <https://tc39.es/ecma262/#sec-identifiers-static-semantics-early-errors>.
    fn parse(
        self,
        cursor: &mut Cursor<R>,
        interner: &mut Interner,
    ) -> Result<Self::Output, ParseError> {
        let _timer = Profiler::global().start_event("BindingIdentifier", "Parsing");

        let next_token = cursor.next(interner)?.ok_or(ParseError::AbruptEnd)?;

        match next_token.kind() {
            TokenKind::Identifier(Sym::ARGUMENTS) if cursor.strict_mode() => {
                Err(ParseError::lex(LexError::Syntax(
                    "unexpected identifier 'arguments' in strict mode".into(),
                    next_token.span().start(),
                )))
            }
            TokenKind::Identifier(Sym::EVAL) if cursor.strict_mode() => {
                Err(ParseError::lex(LexError::Syntax(
                    "unexpected identifier 'eval' in strict mode".into(),
                    next_token.span().start(),
                )))
            }
            TokenKind::Keyword((Keyword::Let, _)) if cursor.strict_mode() => {
                Err(ParseError::lex(LexError::Syntax(
                    "unexpected identifier 'let' in strict mode".into(),
                    next_token.span().start(),
                )))
            }
            TokenKind::Keyword((Keyword::Let, _)) => Ok(Sym::LET),
            TokenKind::Identifier(ref s) => Ok(*s),
            TokenKind::Keyword((Keyword::Yield, _)) if self.allow_yield.0 => {
                // Early Error: It is a Syntax Error if this production has a [Yield] parameter and StringValue of Identifier is "yield".
                Err(ParseError::general(
                    "Unexpected identifier",
                    next_token.span().start(),
                ))
            }
            TokenKind::Keyword((Keyword::Yield, _)) if !self.allow_yield.0 => {
                if cursor.strict_mode() {
                    Err(ParseError::general(
                        "yield keyword in binding identifier not allowed in strict mode",
                        next_token.span().start(),
                    ))
                } else {
                    Ok(Sym::YIELD)
                }
            }
            TokenKind::Keyword((Keyword::Await, _)) if cursor.arrow() => Ok(Sym::AWAIT),
            TokenKind::Keyword((Keyword::Await, _)) if self.allow_await.0 => {
                // Early Error: It is a Syntax Error if this production has an [Await] parameter and StringValue of Identifier is "await".
                Err(ParseError::general(
                    "Unexpected identifier",
                    next_token.span().start(),
                ))
            }
            TokenKind::Keyword((Keyword::Await, _)) if !self.allow_await.0 => {
                if cursor.strict_mode() {
                    Err(ParseError::general(
                        "await keyword in binding identifier not allowed in strict mode",
                        next_token.span().start(),
                    ))
                } else {
                    Ok(Sym::AWAIT)
                }
            }
            _ => Err(ParseError::expected(
                ["identifier".to_owned()],
                next_token.to_string(interner),
                next_token.span(),
                "binding identifier",
            )),
        }
    }
}

/// `ObjectBindingPattern` pattern parsing.
///
/// More information:
///  - [ECMAScript specification][spec]
///
/// [spec]: https://tc39.es/ecma262/#prod-ObjectBindingPattern
#[derive(Debug, Clone, Copy)]
pub(super) struct ObjectBindingPattern {
    allow_in: AllowIn,
    allow_yield: AllowYield,
    allow_await: AllowAwait,
}

impl ObjectBindingPattern {
    /// Creates a new `ObjectBindingPattern` parser.
    pub(super) fn new<I, Y, A>(allow_in: I, allow_yield: Y, allow_await: A) -> Self
    where
        I: Into<AllowIn>,
        Y: Into<AllowYield>,
        A: Into<AllowAwait>,
    {
        Self {
            allow_in: allow_in.into(),
            allow_yield: allow_yield.into(),
            allow_await: allow_await.into(),
        }
    }
}

impl<R> TokenParser<R> for ObjectBindingPattern
where
    R: Read,
{
    type Output = Vec<BindingPatternTypeObject>;

    fn parse(
        self,
        cursor: &mut Cursor<R>,
        interner: &mut Interner,
    ) -> Result<Self::Output, ParseError> {
        let _timer = Profiler::global().start_event("ObjectBindingPattern", "Parsing");

        cursor.expect(
            TokenKind::Punctuator(Punctuator::OpenBlock),
            "object binding pattern",
            interner,
        )?;

        let mut patterns = Vec::new();
        let mut property_names = Vec::new();
        let mut rest_property_name = None;

        loop {
            let next_token_is_colon = *cursor
                .peek(1, interner)?
                .ok_or(ParseError::AbruptEnd)?
                .kind()
                == TokenKind::Punctuator(Punctuator::Colon);
            let token = cursor.peek(0, interner)?.ok_or(ParseError::AbruptEnd)?;
            match token.kind() {
                TokenKind::Punctuator(Punctuator::CloseBlock) => {
                    cursor.expect(
                        TokenKind::Punctuator(Punctuator::CloseBlock),
                        "object binding pattern",
                        interner,
                    )?;
                    break;
                }
                TokenKind::Punctuator(Punctuator::Spread) => {
                    cursor.expect(
                        TokenKind::Punctuator(Punctuator::Spread),
                        "object binding pattern",
                        interner,
                    )?;
                    rest_property_name = Some(
                        BindingIdentifier::new(self.allow_yield, self.allow_await)
                            .parse(cursor, interner)?,
                    );
                    cursor.expect(
                        TokenKind::Punctuator(Punctuator::CloseBlock),
                        "object binding pattern",
                        interner,
                    )?;
                    break;
                }
                _ => {
                    let is_property_name = match token.kind() {
                        TokenKind::Punctuator(Punctuator::OpenBracket)
                        | TokenKind::StringLiteral(_)
                        | TokenKind::NumericLiteral(_) => true,
                        TokenKind::Identifier(_) if next_token_is_colon => true,
                        TokenKind::Keyword(_) if next_token_is_colon => true,
                        _ => false,
                    };

                    if is_property_name {
                        let property_name = PropertyName::new(self.allow_yield, self.allow_await)
                            .parse(cursor, interner)?;
                        if let Some(name) = property_name.prop_name() {
                            property_names.push(name);
                        }
                        cursor.expect(
                            TokenKind::Punctuator(Punctuator::Colon),
                            "object binding pattern",
                            interner,
                        )?;
                        if let Some(peek_token) = cursor.peek(0, interner)? {
                            match peek_token.kind() {
                                TokenKind::Punctuator(Punctuator::OpenBlock) => {
                                    let bindings = Self::new(
                                        self.allow_in,
                                        self.allow_yield,
                                        self.allow_await,
                                    )
                                    .parse(cursor, interner)?;

                                    if let Some(peek_token) = cursor.peek(0, interner)? {
                                        match peek_token.kind() {
                                            TokenKind::Punctuator(Punctuator::Assign) => {
                                                let init = Initializer::new(
                                                    None,
                                                    self.allow_in,
                                                    self.allow_yield,
                                                    self.allow_await,
                                                )
                                                .parse(cursor, interner)?;
                                                patterns.push(
                                                    BindingPatternTypeObject::BindingPattern {
                                                        ident: property_name,
                                                        pattern: DeclarationPattern::Object(
                                                            DeclarationPatternObject::new(
                                                                bindings, None,
                                                            ),
                                                        ),
                                                        default_init: Some(init),
                                                    },
                                                );
                                            }
                                            _ => {
                                                patterns.push(
                                                    BindingPatternTypeObject::BindingPattern {
                                                        ident: property_name,
                                                        pattern: DeclarationPattern::Object(
                                                            DeclarationPatternObject::new(
                                                                bindings, None,
                                                            ),
                                                        ),
                                                        default_init: None,
                                                    },
                                                );
                                            }
                                        }
                                    }
                                }
                                TokenKind::Punctuator(Punctuator::OpenBracket) => {
                                    let bindings = ArrayBindingPattern::new(
                                        self.allow_in,
                                        self.allow_yield,
                                        self.allow_await,
                                    )
                                    .parse(cursor, interner)?;

                                    if let Some(peek_token) = cursor.peek(0, interner)? {
                                        match peek_token.kind() {
                                            TokenKind::Punctuator(Punctuator::Assign) => {
                                                let init = Initializer::new(
                                                    None,
                                                    self.allow_in,
                                                    self.allow_yield,
                                                    self.allow_await,
                                                )
                                                .parse(cursor, interner)?;
                                                patterns.push(
                                                    BindingPatternTypeObject::BindingPattern {
                                                        ident: property_name,
                                                        pattern: DeclarationPattern::Array(
                                                            DeclarationPatternArray::new(
                                                                bindings, None,
                                                            ),
                                                        ),
                                                        default_init: Some(init),
                                                    },
                                                );
                                            }
                                            _ => {
                                                patterns.push(
                                                    BindingPatternTypeObject::BindingPattern {
                                                        ident: property_name,
                                                        pattern: DeclarationPattern::Array(
                                                            DeclarationPatternArray::new(
                                                                bindings, None,
                                                            ),
                                                        ),
                                                        default_init: None,
                                                    },
                                                );
                                            }
                                        }
                                    }
                                }
                                _ => {
                                    // TODO: Currently parses only BindingIdentifier.
                                    //       Should parse https://tc39.es/ecma262/#prod-PropertyName
                                    let ident =
                                        BindingIdentifier::new(self.allow_yield, self.allow_await)
                                            .parse(cursor, interner)?;

                                    if let Some(peek_token) = cursor.peek(0, interner)? {
                                        match peek_token.kind() {
                                            TokenKind::Punctuator(Punctuator::Assign) => {
                                                let init = Initializer::new(
                                                    None,
                                                    self.allow_in,
                                                    self.allow_yield,
                                                    self.allow_await,
                                                )
                                                .parse(cursor, interner)?;
                                                patterns.push(
                                                    BindingPatternTypeObject::SingleName {
                                                        ident,
                                                        property_name,
                                                        default_init: Some(init),
                                                    },
                                                );
                                            }
                                            _ => {
                                                patterns.push(
                                                    BindingPatternTypeObject::SingleName {
                                                        ident,
                                                        property_name,
                                                        default_init: None,
                                                    },
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        let name = BindingIdentifier::new(self.allow_yield, self.allow_await)
                            .parse(cursor, interner)?;
                        property_names.push(name);
                        match cursor.peek(0, interner)?.map(Token::kind) {
                            Some(TokenKind::Punctuator(Punctuator::Assign)) => {
                                let init = Initializer::new(
                                    Some(name),
                                    self.allow_in,
                                    self.allow_yield,
                                    self.allow_await,
                                )
                                .parse(cursor, interner)?;
                                patterns.push(BindingPatternTypeObject::SingleName {
                                    ident: name,
                                    property_name: name.into(),
                                    default_init: Some(init),
                                });
                            }
                            _ => {
                                patterns.push(BindingPatternTypeObject::SingleName {
                                    ident: name,
                                    property_name: name.into(),
                                    default_init: None,
                                });
                            }
                        }
                    }
                }
            }

            if let Some(peek_token) = cursor.peek(0, interner)? {
                if let TokenKind::Punctuator(Punctuator::Comma) = peek_token.kind() {
                    cursor.expect(
                        TokenKind::Punctuator(Punctuator::Comma),
                        "object binding pattern",
                        interner,
                    )?;
                }
            }
        }

        if let Some(rest) = rest_property_name {
            if patterns.is_empty() {
                Ok(vec![BindingPatternTypeObject::RestProperty {
                    ident: rest,
                    excluded_keys: property_names,
                }])
            } else {
                patterns.push(BindingPatternTypeObject::RestProperty {
                    ident: rest,
                    excluded_keys: property_names,
                });
                Ok(patterns)
            }
        } else if patterns.is_empty() {
            Ok(vec![BindingPatternTypeObject::Empty])
        } else {
            Ok(patterns)
        }
    }
}

/// `ArrayBindingPattern` pattern parsing.
///
/// More information:
///  - [ECMAScript specification][spec]
///
/// [spec]: https://tc39.es/ecma262/#prod-ArrayBindingPattern
#[derive(Debug, Clone, Copy)]
pub(super) struct ArrayBindingPattern {
    allow_in: AllowIn,
    allow_yield: AllowYield,
    allow_await: AllowAwait,
}

impl ArrayBindingPattern {
    /// Creates a new `ArrayBindingPattern` parser.
    pub(super) fn new<I, Y, A>(allow_in: I, allow_yield: Y, allow_await: A) -> Self
    where
        I: Into<AllowIn>,
        Y: Into<AllowYield>,
        A: Into<AllowAwait>,
    {
        Self {
            allow_in: allow_in.into(),
            allow_yield: allow_yield.into(),
            allow_await: allow_await.into(),
        }
    }
}

impl<R> TokenParser<R> for ArrayBindingPattern
where
    R: Read,
{
    type Output = Vec<BindingPatternTypeArray>;

    fn parse(
        self,
        cursor: &mut Cursor<R>,
        interner: &mut Interner,
    ) -> Result<Self::Output, ParseError> {
        let _timer = Profiler::global().start_event("ArrayBindingPattern", "Parsing");

        cursor.expect(
            TokenKind::Punctuator(Punctuator::OpenBracket),
            "array binding pattern",
            interner,
        )?;

        let mut patterns = Vec::new();
        let mut last_elision_or_first = true;

        loop {
            match cursor
                .peek(0, interner)?
                .ok_or(ParseError::AbruptEnd)?
                .kind()
            {
                TokenKind::Punctuator(Punctuator::CloseBracket) => {
                    cursor.expect(
                        TokenKind::Punctuator(Punctuator::CloseBracket),
                        "array binding pattern",
                        interner,
                    )?;
                    break;
                }
                TokenKind::Punctuator(Punctuator::Comma) => {
                    cursor.expect(
                        TokenKind::Punctuator(Punctuator::Comma),
                        "array binding pattern",
                        interner,
                    )?;
                    if last_elision_or_first {
                        patterns.push(BindingPatternTypeArray::Elision);
                    } else {
                        last_elision_or_first = true;
                    }
                    continue;
                }
                TokenKind::Punctuator(Punctuator::Spread) => {
                    cursor.expect(
                        TokenKind::Punctuator(Punctuator::Spread),
                        "array binding pattern",
                        interner,
                    )?;

                    match cursor
                        .peek(0, interner)?
                        .ok_or(ParseError::AbruptEnd)?
                        .kind()
                    {
                        TokenKind::Punctuator(Punctuator::OpenBlock) => {
                            let bindings = ObjectBindingPattern::new(
                                self.allow_in,
                                self.allow_yield,
                                self.allow_await,
                            )
                            .parse(cursor, interner)?;
                            patterns.push(BindingPatternTypeArray::BindingPatternRest {
                                pattern: DeclarationPattern::Object(DeclarationPatternObject::new(
                                    bindings, None,
                                )),
                            });
                        }
                        TokenKind::Punctuator(Punctuator::OpenBracket) => {
                            let bindings =
                                Self::new(self.allow_in, self.allow_yield, self.allow_await)
                                    .parse(cursor, interner)?;
                            patterns.push(BindingPatternTypeArray::BindingPatternRest {
                                pattern: DeclarationPattern::Array(DeclarationPatternArray::new(
                                    bindings, None,
                                )),
                            });
                        }
                        _ => {
                            let rest_property_name =
                                BindingIdentifier::new(self.allow_yield, self.allow_await)
                                    .parse(cursor, interner)?;
                            patterns.push(BindingPatternTypeArray::SingleNameRest {
                                ident: rest_property_name,
                            });
                        }
                    }

                    cursor.expect(
                        TokenKind::Punctuator(Punctuator::CloseBracket),
                        "array binding pattern",
                        interner,
                    )?;
                    break;
                }
                TokenKind::Punctuator(Punctuator::OpenBlock) => {
                    last_elision_or_first = false;

                    let bindings = ObjectBindingPattern::new(
                        self.allow_in,
                        self.allow_yield,
                        self.allow_await,
                    )
                    .parse(cursor, interner)?;

                    match cursor
                        .peek(0, interner)?
                        .ok_or(ParseError::AbruptEnd)?
                        .kind()
                    {
                        TokenKind::Punctuator(Punctuator::Assign) => {
                            let default_init = Initializer::new(
                                None,
                                self.allow_in,
                                self.allow_yield,
                                self.allow_await,
                            )
                            .parse(cursor, interner)?;
                            patterns.push(BindingPatternTypeArray::BindingPattern {
                                pattern: DeclarationPattern::Object(DeclarationPatternObject::new(
                                    bindings,
                                    Some(default_init),
                                )),
                            });
                        }
                        _ => {
                            patterns.push(BindingPatternTypeArray::BindingPattern {
                                pattern: DeclarationPattern::Object(DeclarationPatternObject::new(
                                    bindings, None,
                                )),
                            });
                        }
                    }
                }
                TokenKind::Punctuator(Punctuator::OpenBracket) => {
                    last_elision_or_first = false;

                    let bindings = Self::new(self.allow_in, self.allow_yield, self.allow_await)
                        .parse(cursor, interner)?;

                    match cursor
                        .peek(0, interner)?
                        .ok_or(ParseError::AbruptEnd)?
                        .kind()
                    {
                        TokenKind::Punctuator(Punctuator::Assign) => {
                            let default_init = Initializer::new(
                                None,
                                self.allow_in,
                                self.allow_yield,
                                self.allow_await,
                            )
                            .parse(cursor, interner)?;
                            patterns.push(BindingPatternTypeArray::BindingPattern {
                                pattern: DeclarationPattern::Array(DeclarationPatternArray::new(
                                    bindings,
                                    Some(default_init),
                                )),
                            });
                        }
                        _ => {
                            patterns.push(BindingPatternTypeArray::BindingPattern {
                                pattern: DeclarationPattern::Array(DeclarationPatternArray::new(
                                    bindings, None,
                                )),
                            });
                        }
                    }
                }
                _ => {
                    last_elision_or_first = false;

                    let ident = BindingIdentifier::new(self.allow_yield, self.allow_await)
                        .parse(cursor, interner)?;
                    match cursor
                        .peek(0, interner)?
                        .ok_or(ParseError::AbruptEnd)?
                        .kind()
                    {
                        TokenKind::Punctuator(Punctuator::Assign) => {
                            let default_init = Initializer::new(
                                Some(ident),
                                self.allow_in,
                                self.allow_yield,
                                self.allow_await,
                            )
                            .parse(cursor, interner)?;
                            patterns.push(BindingPatternTypeArray::SingleName {
                                ident,
                                default_init: Some(default_init),
                            });
                        }
                        _ => {
                            patterns.push(BindingPatternTypeArray::SingleName {
                                ident,
                                default_init: None,
                            });
                        }
                    }
                }
            }

            if let Some(peek_token) = cursor.peek(0, interner)? {
                if let TokenKind::Punctuator(Punctuator::Comma) = peek_token.kind() {
                    cursor.expect(
                        TokenKind::Punctuator(Punctuator::Comma),
                        "array binding pattern",
                        interner,
                    )?;
                    if last_elision_or_first {
                        patterns.push(BindingPatternTypeArray::Elision);
                    } else {
                        last_elision_or_first = true;
                    }
                }
            }
        }

        Ok(patterns)
    }
}
