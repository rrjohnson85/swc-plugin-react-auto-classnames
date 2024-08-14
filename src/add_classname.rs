use std::path::Path;

use swc_core::common::DUMMY_SP;
use swc_core::ecma::ast::{
    BlockStmtOrExpr, Expr, Ident, JSXAttr, JSXAttrName, JSXAttrOrSpread, JSXAttrValue,
    JSXElementName, JSXExpr, JSXExprContainer, JSXOpeningElement, Lit, Stmt, Str,
};
use swc_core::ecma::atoms::js_word;
use swc_core::ecma::visit::{VisitMut, VisitMutWith};

#[derive(Default)]
pub struct AddClassnameVisitor<'a> {
    filename: &'a str,
}

impl<'a> AddClassnameVisitor<'a> {
    pub fn new(file_path: &'a str) -> Self {
        let path = Path::new(file_path);
        let filename: &str = path.file_stem().and_then(|stem| stem.to_str()).unwrap();

        AddClassnameVisitor { filename }
    }

    fn class_name(&self, component_name: &str) -> String {
        format!(
            "{}-{}",
            self.camel_to_hyphen_case(self.filename),
            self.camel_to_hyphen_case(component_name)
        )
    }

    fn camel_to_hyphen_case(&self, camel_case: &str) -> String {
        let mut result: String = String::new();
        let mut prev_char_was_lowercase: bool = false;

        for (i, c) in camel_case.chars().enumerate() {
            if c == '_' {
                result.push('-');
            } else if i > 0 && c.is_uppercase() {
                if prev_char_was_lowercase {
                    result.push('-');
                }
                result.push(c.to_lowercase().next().unwrap());
            } else {
                result.push(c.to_lowercase().next().unwrap());
            }
            prev_char_was_lowercase = c.is_lowercase();
        }
        result
    }
}

impl<'a> VisitMut for AddClassnameVisitor<'a> {
    /**
     * The VisitMut trait is used to traverse the AST and modify it in place.
     * visit_mut_jsx_opening_element is called when the visitor encounters a tag in the JSX.
     * We add the className attribute to the React node for it to be converted to a CSS class.
     */
    fn visit_mut_jsx_opening_element(&mut self, n: &mut JSXOpeningElement) {
        let component_name = match &n.name {
            JSXElementName::Ident(ident) => ident.sym.to_string(),
            JSXElementName::JSXMemberExpr(expr) => match &expr.prop {
                Ident { sym, .. } => sym.to_string(),
            },
            _ => return,
        };

        if component_name.contains("Fragment") || component_name.ends_with("Provider") {
            return;
        }

        let class_name: String = self.class_name(&component_name);

        let has_class_name = n.attrs.iter_mut().any(|attr| match attr {
            JSXAttrOrSpread::JSXAttr(JSXAttr { name, value, .. }) => {
                if let JSXAttrName::Ident(ident) = name {
                    // If you find the className attribute, append to it
                    if ident.sym == js_word!("className") {
                        if let Some(JSXAttrValue::Lit(Lit::Str(existing_value))) = value {
                            let new_value = Lit::Str(Str {
                                span: DUMMY_SP,
                                value: format!("{} {}", existing_value.value, class_name).into(),
                                raw: None,
                            });
                            *value = Some(JSXAttrValue::Lit(new_value));
                        }
                        return true;
                    }

                    // If the value of a prop is a function that returns JSX or a JSXComponent
                    // we need to visit it to add the className attribute
                    if let Some(JSXAttrValue::JSXExprContainer(expr_container)) = value {
                        self.visit_mut_jsx_expr_container(expr_container);
                    }
                }
                false
            }
            _ => false,
        });

        if !has_class_name {
            n.attrs.insert(0, JSXAttrOrSpread::JSXAttr(JSXAttr {
                span: DUMMY_SP,
                name: JSXAttrName::Ident(Ident::new(js_word!("className"), DUMMY_SP)),
                value: Some(JSXAttrValue::Lit(Lit::Str(Str {
                    span: DUMMY_SP,
                    value: class_name.into(),
                    raw: None,
                }))),
            }));
        }
    }

    fn visit_mut_jsx_expr_container(&mut self, expr_container: &mut JSXExprContainer) {
        match &mut expr_container.expr {
            JSXExpr::Expr(expr) => {
                if let Expr::Arrow(arrow_expr) = &mut **expr {
                    match &mut *arrow_expr.body {
                        BlockStmtOrExpr::Expr(inner_expr) => {
                            // Adjusted handling for boxed expressions.
                            // Dereference the boxed expression to inspect it.
                            if let Expr::JSXElement(element) = &mut **inner_expr {
                                element.visit_mut_with(self);
                            }
                        }
                        BlockStmtOrExpr::BlockStmt(block_stmt) => {
                            // Iterate over statements in block statement for return statements.
                            for stmt in &mut block_stmt.stmts {
                                if let Stmt::Return(return_stmt) = stmt {
                                    if let Some(returned_expr) = &mut return_stmt.arg {
                                        // Again, properly dereference the boxed expression to inspect it.
                                        if let Expr::JSXElement(element) = &mut **returned_expr {
                                            element.visit_mut_with(self);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        expr_container.visit_mut_children_with(self);
    }
}
