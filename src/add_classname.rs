use std::path::Path;

use swc_core::common::{SyntaxContext, DUMMY_SP};
use swc_core::ecma::ast::{
    op, BinExpr, BlockStmtOrExpr, Expr, Ident, IdentName, JSXAttr, JSXAttrName, JSXAttrOrSpread,
    JSXAttrValue, JSXElementName, JSXExpr, JSXExprContainer, JSXOpeningElement, Lit, MemberExpr,
    MemberProp, OptChainBase, OptChainExpr, SpreadElement, Stmt, Str, Tpl, TplElement,
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

    fn debug(&self, label: &str, obj: &dyn std::fmt::Debug) {
        if std::env::var("DEBUG").is_ok() {
            println!("{}: {:?}", label, obj);
        }
    }
}

impl VisitMut for AddClassnameVisitor<'_> {
    /**
     * The VisitMut trait is used to traverse the AST and modify it in place.
     * visit_mut_jsx_opening_element is called when the visitor encounters a tag in the JSX.
     * We add the className attribute to the React node for it to be converted to a CSS class.
     */
    fn visit_mut_jsx_opening_element(&mut self, n: &mut JSXOpeningElement) {
        let component_name = match &n.name {
            JSXElementName::Ident(ident) => ident.sym.to_string(),
            JSXElementName::JSXMemberExpr(expr) => {
                let IdentName { sym, .. } = &expr.prop;
                sym.to_string()
            }
            _ => return,
        };

        if component_name.contains("Fragment") || component_name.ends_with("Provider") {
            return;
        }

        let mut spread_identifier = "".to_string();

        n.attrs.iter().find(|attr| match attr {
            JSXAttrOrSpread::SpreadElement(SpreadElement {
                dot3_token: _,
                expr,
            }) => {
                // if expr is identifier and the sym is props
                if let Expr::Ident(ident) = &**expr {
                    // if the spread element is props, we don't need to add className
                    self.debug("Found spread element:", expr);
                    spread_identifier = ident.sym.to_string();
                }
                false
            }
            _ => false,
        });

        let class_name: String = self.class_name(&component_name);

        let has_class_name = n.attrs.iter_mut().any(|attr| match attr {
            JSXAttrOrSpread::JSXAttr(JSXAttr { name, value, .. }) => {
                if let JSXAttrName::Ident(ident) = name {
                    if ident.sym == js_word!("className") {
                        if let Some(JSXAttrValue::Lit(Lit::Str(existing_value))) = value {
                            // className="some-class" should become className="class_name some-class"
                            self.debug("Found className string: {:?}", existing_value);

                            let new_value = Lit::Str(Str {
                                span: DUMMY_SP,
                                value: format!("{} {}", class_name, existing_value.value).into(),
                                raw: None,
                            });
                            *value = Some(JSXAttrValue::Lit(new_value));
                        }
                        if let Some(JSXAttrValue::JSXExprContainer(expr_container)) = value {
                            if let JSXExpr::Expr(expr) = &mut expr_container.expr {
                                if let Expr::Tpl(tpl) = &mut **expr {
                                    // className={`some-${value}`} should become className={`class_name (some-${value})`}
                                    self.debug("Found template literal: {:?}", tpl);

                                    let start_quasi: &TplElement = tpl.quasis.first().unwrap();
                                    let new_start_quasi: TplElement = TplElement {
                                        span: DUMMY_SP,
                                        tail: start_quasi.tail,
                                        cooked: Option::Some(
                                            format!("{} {}", class_name, start_quasi.raw).into(),
                                        ),
                                        raw: format!("{} {}", class_name, start_quasi.raw).into(),
                                    };
                                    tpl.quasis.splice(0..1, vec![new_start_quasi]);
                                }
                                if let Expr::Bin(bin_expr) = &mut **expr {
                                    // className={value + ' '} should become className={class_name + value + ' '}
                                    self.debug("Found binary expression: {:?}", bin_expr);

                                    bin_expr.right = Box::new(Expr::Bin(bin_expr.clone()));
                                    bin_expr.left = Box::new(Expr::Lit(Lit::Str(Str {
                                        span: DUMMY_SP,
                                        value: format!("{} ", class_name).into(),
                                        raw: None,
                                    })));
                                }
                            }
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
            let attribute_name = JSXAttrName::Ident(
                Ident::new(js_word!("className"), DUMMY_SP, SyntaxContext::empty()).into(),
            );

            if !spread_identifier.is_empty() {
                n.attrs.push(JSXAttrOrSpread::JSXAttr(JSXAttr {
                    span: DUMMY_SP,
                    name: attribute_name,
                    value: Some(JSXAttrValue::JSXExprContainer(JSXExprContainer {
                        span: DUMMY_SP,
                        expr: JSXExpr::Expr(Box::new(Expr::Tpl(Tpl {
                            span: DUMMY_SP,
                            quasis: vec![
                                TplElement {
                                    span: DUMMY_SP,
                                    tail: false,
                                    cooked: Some(format!("{} ", class_name.clone()).into()),
                                    raw: format!("{} ", class_name.clone()).into(),
                                },
                                TplElement {
                                    span: DUMMY_SP,
                                    tail: true,
                                    cooked: Some("".into()),
                                    raw: "".into(),
                                },
                            ],
                            exprs: vec![Box::new(Expr::Bin(BinExpr {
                                span: DUMMY_SP,
                                op: op!("||"),
                                left: Box::new(Expr::OptChain(OptChainExpr {
                                    span: DUMMY_SP,
                                    optional: true,
                                    base: Box::new(OptChainBase::Member(MemberExpr {
                                        span: DUMMY_SP,
                                        obj: Box::new(Expr::Ident(Ident {
                                            span: DUMMY_SP,
                                            sym: spread_identifier.clone().into(),
                                            optional: false,
                                            ctxt: SyntaxContext::empty(),
                                        })),
                                        prop: MemberProp::Ident(
                                            Ident {
                                                span: DUMMY_SP,
                                                sym: "className".into(),
                                                optional: false,
                                                ctxt: SyntaxContext::empty(),
                                            }
                                            .into(),
                                        ),
                                    })),
                                })),
                                right: Box::new(Expr::Lit(Lit::Str(Str {
                                    span: DUMMY_SP,
                                    value: "".into(),
                                    raw: None,
                                }))),
                            }))],
                        }))),
                    })),
                }));
            } else {
                // <Component otherProp="value" /> should become <Component className="class_name" otherProp="value" />
                n.attrs.insert(
                    0,
                    JSXAttrOrSpread::JSXAttr(JSXAttr {
                        span: DUMMY_SP,
                        name: attribute_name,
                        value: Some(JSXAttrValue::Lit(Lit::Str(Str {
                            span: DUMMY_SP,
                            value: class_name.into(),
                            raw: None,
                        }))),
                    }),
                );
            }
        }
    }

    fn visit_mut_jsx_expr_container(&mut self, expr_container: &mut JSXExprContainer) {
        if let JSXExpr::Expr(expr) = &mut expr_container.expr {
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
        expr_container.visit_mut_children_with(self);
    }
}
