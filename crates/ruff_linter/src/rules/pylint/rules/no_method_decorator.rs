use std::collections::HashMap;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_trivia::indentation_at_offset;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix;

/// ## What it does
/// Checks for the use of a classmethod being made without the decorator.
///
/// ## Why is this bad?
/// When it comes to consistency and readability, it's preferred to use the decorator.
///
/// ## Example
///
/// ```python
/// class Foo:
///     def bar(cls): ...
///
///     bar = classmethod(bar)
/// ```
///
/// Use instead:
///
/// ```python
/// class Foo:
///     @classmethod
///     def bar(cls): ...
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct NoClassmethodDecorator;

impl AlwaysFixableViolation for NoClassmethodDecorator {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Class method defined without decorator".to_string()
    }

    fn fix_title(&self) -> String {
        "Add @classmethod decorator".to_string()
    }
}

/// ## What it does
/// Checks for the use of a staticmethod being made without the decorator.
///
/// ## Why is this bad?
/// When it comes to consistency and readability, it's preferred to use the decorator.
///
/// ## Example
///
/// ```python
/// class Foo:
///     def bar(arg1, arg2): ...
///
///     bar = staticmethod(bar)
/// ```
///
/// Use instead:
///
/// ```python
/// class Foo:
///     @staticmethod
///     def bar(arg1, arg2): ...
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct NoStaticmethodDecorator;

impl AlwaysFixableViolation for NoStaticmethodDecorator {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Static method defined without decorator".to_string()
    }

    fn fix_title(&self) -> String {
        "Add @staticmethod decorator".to_string()
    }
}

enum MethodType {
    Classmethod,
    Staticmethod,
}

/// PLR0202
pub(crate) fn no_classmethod_decorator(checker: &Checker, stmt: &Stmt) {
    get_undecorated_methods(checker, stmt, &MethodType::Classmethod);
}

/// PLR0203
pub(crate) fn no_staticmethod_decorator(checker: &Checker, stmt: &Stmt) {
    get_undecorated_methods(checker, stmt, &MethodType::Staticmethod);
}

fn get_undecorated_methods(checker: &Checker, class_stmt: &Stmt, method_type: &MethodType) {
    let Stmt::ClassDef(class_def) = class_stmt else {
        return;
    };

    let mut explicit_decorator_calls: HashMap<Name, &Stmt> = HashMap::default();

    let method_name = match method_type {
        MethodType::Classmethod => "classmethod",
        MethodType::Staticmethod => "staticmethod",
    };

    // gather all explicit *method calls
    for stmt in &class_def.body {
        if let Stmt::Assign(ast::StmtAssign { targets, value, .. }) = stmt {
            if let Expr::Call(ast::ExprCall {
                func, arguments, ..
            }) = value.as_ref()
            {
                if let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() {
                    if id == method_name && checker.semantic().has_builtin_binding(method_name) {
                        if arguments.args.len() != 1 {
                            continue;
                        }

                        if targets.len() != 1 {
                            continue;
                        }

                        let target_name = match targets.first() {
                            Some(Expr::Name(ast::ExprName { id, .. })) => id.to_string(),
                            _ => continue,
                        };

                        if let Expr::Name(ast::ExprName { id, .. }) = &arguments.args[0] {
                            if target_name == *id {
                                explicit_decorator_calls.insert(id.clone(), stmt);
                            }
                        }
                    }
                }
            }
        }
    }

    if explicit_decorator_calls.is_empty() {
        return;
    }

    for stmt in &class_def.body {
        if let Stmt::FunctionDef(ast::StmtFunctionDef {
            name,
            decorator_list,
            ..
        }) = stmt
        {
            let Some(decorator_call_statement) = explicit_decorator_calls.get(name.id()) else {
                continue;
            };

            // if we find the decorator we're looking for, skip
            if decorator_list.iter().any(|decorator| {
                if let Expr::Name(ast::ExprName { id, .. }) = &decorator.expression {
                    if id == method_name && checker.semantic().has_builtin_binding(method_name) {
                        return true;
                    }
                }

                false
            }) {
                continue;
            }

            let range = TextRange::new(stmt.range().start(), stmt.range().start());
            let mut diagnostic = match method_type {
                MethodType::Classmethod => Diagnostic::new(NoClassmethodDecorator, range),
                MethodType::Staticmethod => Diagnostic::new(NoStaticmethodDecorator, range),
            };

            let indentation = indentation_at_offset(stmt.range().start(), checker.source());

            match indentation {
                Some(indentation) => {
                    diagnostic.set_fix(Fix::safe_edits(
                        Edit::insertion(
                            format!("@{method_name}\n{indentation}"),
                            stmt.range().start(),
                        ),
                        [fix::edits::delete_stmt(
                            decorator_call_statement,
                            Some(class_stmt),
                            checker.locator(),
                            checker.indexer(),
                        )],
                    ));
                    checker.report_diagnostic(diagnostic);
                }
                None => {
                    continue;
                }
            }
        }
    }
}
