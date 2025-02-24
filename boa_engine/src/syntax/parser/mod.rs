//! Boa parser implementation.

mod cursor;
pub mod error;
mod expression;
pub(crate) mod function;
mod statement;
#[cfg(test)]
mod tests;

pub use self::error::{ParseError, ParseResult};

use self::cursor::Cursor;
use crate::{
    syntax::{ast::node::StatementList, lexer::TokenKind},
    Context,
};
use boa_interner::{Interner, Sym};
use rustc_hash::{FxHashMap, FxHashSet};
use std::io::Read;

use super::ast::Position;

/// Trait implemented by parsers.
///
/// This makes it possible to abstract over the underlying implementation of a parser.
trait TokenParser<R>: Sized
where
    R: Read,
{
    /// Output type for the parser.
    type Output; // = Node; waiting for https://github.com/rust-lang/rust/issues/29661

    /// Parses the token stream using the current parser.
    ///
    /// This method needs to be provided by the implementor type.
    fn parse(
        self,
        cursor: &mut Cursor<R>,
        interner: &mut Interner,
    ) -> Result<Self::Output, ParseError>;
}

/// Boolean representing if the parser should allow a `yield` keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AllowYield(bool);

impl From<bool> for AllowYield {
    fn from(allow: bool) -> Self {
        Self(allow)
    }
}

/// Boolean representing if the parser should allow a `await` keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AllowAwait(bool);

impl From<bool> for AllowAwait {
    fn from(allow: bool) -> Self {
        Self(allow)
    }
}

/// Boolean representing if the parser should allow a `in` keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AllowIn(bool);

impl From<bool> for AllowIn {
    fn from(allow: bool) -> Self {
        Self(allow)
    }
}

/// Boolean representing if the parser should allow a `return` keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AllowReturn(bool);

impl From<bool> for AllowReturn {
    fn from(allow: bool) -> Self {
        Self(allow)
    }
}

/// Boolean representing if the parser should allow a `default` keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AllowDefault(bool);

impl From<bool> for AllowDefault {
    fn from(allow: bool) -> Self {
        Self(allow)
    }
}

#[derive(Debug)]
pub struct Parser<R> {
    /// Cursor of the parser, pointing to the lexer and used to get tokens for the parser.
    cursor: Cursor<R>,
}

impl<R> Parser<R> {
    /// Create a new `Parser` with a reader as the input to parse.
    pub fn new(reader: R, strict_mode: bool) -> Self
    where
        R: Read,
    {
        let mut cursor = Cursor::new(reader);
        cursor.set_strict_mode(strict_mode);

        Self { cursor }
    }

    /// Parse the full input as a [ECMAScript Script][spec] into the boa AST representation.
    /// The resulting `StatementList` can be compiled into boa bytecode and executed in the boa vm.
    ///
    /// [spec]: https://tc39.es/ecma262/#prod-Script
    pub fn parse_all(&mut self, context: &mut Context) -> Result<StatementList, ParseError>
    where
        R: Read,
    {
        let statement_list = Script.parse(&mut self.cursor, context.interner_mut())?;

        // It is a Syntax Error if the LexicallyDeclaredNames of ScriptBody contains any duplicate entries.
        // It is a Syntax Error if any element of the LexicallyDeclaredNames of ScriptBody also occurs in the VarDeclaredNames of ScriptBody.
        let mut var_declared_names = FxHashSet::default();
        statement_list.var_declared_names_new(&mut var_declared_names);
        let lexically_declared_names = statement_list.lexically_declared_names();
        let mut lexically_declared_names_map: FxHashMap<Sym, bool> = FxHashMap::default();
        for (name, is_function_declaration) in &lexically_declared_names {
            if let Some(existing_is_function_declaration) = lexically_declared_names_map.get(name) {
                if !(*is_function_declaration && *existing_is_function_declaration) {
                    return Err(ParseError::general(
                        "lexical name declared multiple times",
                        Position::new(1, 1),
                    ));
                }
            }
            lexically_declared_names_map.insert(*name, *is_function_declaration);

            if !is_function_declaration && var_declared_names.contains(name) {
                return Err(ParseError::general(
                    "lexical name declared in var names",
                    Position::new(1, 1),
                ));
            }
            if context.has_binding(*name) {
                return Err(ParseError::general(
                    "lexical name declared multiple times",
                    Position::new(1, 1),
                ));
            }
            if !is_function_declaration {
                let name_str = context.interner().resolve_expect(*name);
                let desc = context
                    .realm
                    .global_property_map
                    .string_property_map()
                    .get(name_str);
                let non_configurable_binding_exists = match desc {
                    Some(desc) => !matches!(desc.configurable(), Some(true)),
                    None => false,
                };
                if non_configurable_binding_exists {
                    return Err(ParseError::general(
                        "lexical name declared in var names",
                        Position::new(1, 1),
                    ));
                }
            }
        }
        for name in var_declared_names {
            if context.has_binding(name) {
                return Err(ParseError::general(
                    "lexical name declared in var names",
                    Position::new(1, 1),
                ));
            }
        }

        Ok(statement_list)
    }
}

/// Parses a full script.
///
/// More information:
///  - [ECMAScript specification][spec]
///
/// [spec]: https://tc39.es/ecma262/#prod-Script
#[derive(Debug, Clone, Copy)]
pub struct Script;

impl<R> TokenParser<R> for Script
where
    R: Read,
{
    type Output = StatementList;

    fn parse(
        self,
        cursor: &mut Cursor<R>,
        interner: &mut Interner,
    ) -> Result<Self::Output, ParseError> {
        match cursor.peek(0, interner)? {
            Some(tok) => {
                let mut strict = false;
                match tok.kind() {
                    // Set the strict mode
                    TokenKind::StringLiteral(string)
                        if interner.resolve_expect(*string) == "use strict" =>
                    {
                        cursor.set_strict_mode(true);
                        strict = true;
                    }
                    _ => {}
                }
                let mut statement_list = ScriptBody.parse(cursor, interner)?;
                statement_list.set_strict(strict);
                Ok(statement_list)
            }
            None => Ok(StatementList::from(Vec::new())),
        }
    }
}

/// Parses a script body.
///
/// More information:
///  - [ECMAScript specification][spec]
///
/// [spec]: https://tc39.es/ecma262/#prod-ScriptBody
#[derive(Debug, Clone, Copy)]
pub struct ScriptBody;

impl<R> TokenParser<R> for ScriptBody
where
    R: Read,
{
    type Output = StatementList;

    fn parse(
        self,
        cursor: &mut Cursor<R>,
        interner: &mut Interner,
    ) -> Result<Self::Output, ParseError> {
        self::statement::StatementList::new(false, false, false, false, &[]).parse(cursor, interner)
    }
}
