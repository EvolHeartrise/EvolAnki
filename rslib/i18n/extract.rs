// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::collections::HashSet;
use std::fmt::Display;
use std::fmt::Write;
use std::marker::PhantomData;

use fluent_syntax::ast::Entry;
use fluent_syntax::ast::Expression;
use fluent_syntax::ast::InlineExpression;
use fluent_syntax::ast::Pattern;
use fluent_syntax::ast::PatternElement;
use fluent_syntax::parser::parse;

use crate::gather::TranslationsByModule;

#[derive(Debug, PartialOrd, Ord, PartialEq, Eq)]
pub struct Module {
    pub name: String,
    pub translations: Vec<Translation>,
}

#[derive(Debug, PartialOrd, Ord, PartialEq, Eq)]
pub struct Translation {
    pub key: String,
    pub text: String,
    pub variables: Vec<Variable>,
}

#[derive(Debug, PartialOrd, Ord, PartialEq, Eq)]
pub struct Variable {
    pub name: String,
    pub kind: VariableKind,
}

#[derive(Debug, PartialOrd, Ord, PartialEq, Eq)]
pub enum VariableKind {
    Int,
    Float,
    String,
    Any,
}

pub fn get_modules(data: &TranslationsByModule) -> Vec<Module> {
    data.iter()
        .map(|(name, ftl_text)| Module {
            name: name.clone(),
            translations: extract_metadata(ftl_text),
        })
        .collect()
}

fn extract_metadata(ftl_text: &str) -> Vec<Translation> {
    let mut output = vec![];
    let res = parse(ftl_text.to_string()).unwrap();
    for entry in res.body {
        if let Entry::Message(m) = entry {
            if let Some(pattern) = m.value {
                let mut visitor = Visitor::default();
                visitor.visit_pattern(&pattern);
                let key = m.id.name.to_string();

                // special case translations that were ported from gettext, and use embedded
                // terms that reference other variables that aren't visible to our visitor
                if key == "statistics-studied-today" {
                    visitor.variables.push("amount".to_string());
                    visitor.variables.push("cards".to_string());
                } else if key == "statistics-average-answer-time" {
                    visitor.variables.push("cards-per-minute".to_string());
                }

                let (text, variables) = visitor.into_output();

                output.push(Translation {
                    key,
                    text,
                    variables,
                })
            }
        }
    }
    output.sort_unstable();

    output
}

/// Gather variable names and (rough) text from Fluent AST.
#[derive(Default)]
struct Visitor<T> {
    text: String,
    variables: Vec<String>,
    _phantom: PhantomData<T>,
}

impl<T> Visitor<T>
where
    T: AsRef<str> + Display,
{
    fn into_output(self) -> (String, Vec<Variable>) {
        // make unique, preserving order
        let mut seen = HashSet::new();
        let vars: Vec<_> = self
            .variables
            .into_iter()
            .filter(|v| {
                if seen.contains(v) {
                    false
                } else {
                    seen.insert(v.clone())
                }
            })
            .map(Into::into)
            .collect();
        (self.text, vars)
    }

    fn visit_pattern(&mut self, pattern: &Pattern<T>) {
        for element in &pattern.elements {
            match element {
                PatternElement::TextElement { value } => self.text.push_str(value.as_ref()),
                PatternElement::Placeable { expression } => self.visit_expression(expression),
            }
        }
    }

    fn visit_inline_expression(&mut self, expr: &InlineExpression<T>, in_select: bool) {
        match expr {
            InlineExpression::VariableReference { id } => {
                if !in_select {
                    write!(self.text, "{{${}}}", id.name).unwrap();
                }
                self.variables.push(id.name.to_string());
            }
            InlineExpression::Placeable { expression } => {
                self.visit_expression(expression);
            }
            _ => {}
        }
    }

    fn visit_expression(&mut self, expression: &Expression<T>) {
        match expression {
            Expression::Select { selector, variants } => {
                self.visit_inline_expression(selector, true);
                self.visit_pattern(&variants.last().unwrap().value)
            }
            Expression::Inline(expr) => self.visit_inline_expression(expr, false),
        }
    }
}

impl From<String> for Variable {
    fn from(name: String) -> Self {
        // rather than adding more items here as we add new strings, we should probably
        // try to either reuse existing ones, or consider some sort of Hungarian
        // notation
        let kind = match name.as_str() {
            "cards" | "notes" | "count" | "amount" | "reviews" | "total" | "selected"
            | "kilobytes" | "daysStart" | "daysEnd" | "days" | "secs-per-card" | "remaining"
            | "hourStart" | "hourEnd" | "correct" | "decks" | "changed" => VariableKind::Int,
            "average-seconds" | "cards-per-minute" => VariableKind::Float,
            "val" | "found" | "expected" | "part" | "percent" | "day" | "number" | "up"
            | "down" | "seconds" | "megs" => VariableKind::Any,
            term => {
                let term = term.to_ascii_lowercase();
                if term.ends_with("count") {
                    VariableKind::Int
                } else if term.starts_with("num") {
                    VariableKind::Any
                } else {
                    VariableKind::String
                }
            }
        };
        Variable { name, kind }
    }
}
