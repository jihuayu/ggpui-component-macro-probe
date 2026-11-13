#![forbid(unsafe_code)]
//! Deliberately small view DSL, backed by an actual reactive runtime.
use proc_macro::TokenStream;
use proc_macro2::{TokenStream as Tokens, TokenTree};
use quote::{format_ident, quote};
use std::collections::BTreeMap;
use syn::{
    Expr, FnArg, Ident, ItemFn, Pat, Result, Token, braced, parenthesized,
    parse::{Parse, ParseStream},
    parse_macro_input,
    visit::{self, Visit},
};

mod kw {
    syn::custom_keyword!(key);
}

#[proc_macro_attribute]
pub fn component(attr: TokenStream, input: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return syn::Error::new(proc_macro2::Span::call_site(), "component takes no options")
            .into_compile_error()
            .into();
    }
    let item = parse_macro_input!(input as ItemFn);
    match expand_component(item) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

fn expand_component(item: ItemFn) -> Result<Tokens> {
    if item.sig.asyncness.is_some() || !item.sig.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &item.sig,
            "probe components must be synchronous and non-generic",
        ));
    }
    let mut hooks = HookCheck::default();
    hooks.visit_block(&item.block);
    if let Some(error) = hooks.error {
        return Err(error);
    }
    let mut args = Vec::new();
    let mut types = Vec::new();
    for arg in &item.sig.inputs {
        let FnArg::Typed(arg) = arg else {
            return Err(syn::Error::new_spanned(arg, "methods are not supported"));
        };
        let Pat::Ident(pat) = arg.pat.as_ref() else {
            return Err(syn::Error::new_spanned(arg, "use named component props"));
        };
        args.push(pat.ident.clone());
        types.push(arg.ty.clone());
    }
    let ItemFn {
        attrs,
        vis,
        sig,
        block,
    } = item;
    let name = &sig.ident;
    Ok(quote! {
        #(#attrs)*
        #[allow(non_snake_case)]
        #vis #sig {
            ::probe_ui::View::component(stringify!(#name), move || {
                #(let #args = #args.clone();)*
                #block
            })
        }
        #[allow(non_snake_case, unused_imports)]
        #vis mod #name {
            use super::*;
            pub struct Props { #(pub #args: #types,)* }
            pub fn mount(props: Props) -> ::probe_ui::View {
                super::#name(#(props.#args),*)
            }
        }
    })
}

#[derive(Default)]
struct HookCheck {
    nested: usize,
    error: Option<syn::Error>,
}
impl<'ast> Visit<'ast> for HookCheck {
    fn visit_expr_if(&mut self, node: &'ast syn::ExprIf) {
        self.nested += 1;
        visit::visit_expr_if(self, node);
        self.nested -= 1;
    }
    fn visit_expr_match(&mut self, node: &'ast syn::ExprMatch) {
        self.nested += 1;
        visit::visit_expr_match(self, node);
        self.nested -= 1;
    }
    fn visit_expr_for_loop(&mut self, node: &'ast syn::ExprForLoop) {
        self.nested += 1;
        visit::visit_expr_for_loop(self, node);
        self.nested -= 1;
    }
    fn visit_expr_while(&mut self, node: &'ast syn::ExprWhile) {
        self.nested += 1;
        visit::visit_expr_while(self, node);
        self.nested -= 1;
    }
    fn visit_expr_loop(&mut self, node: &'ast syn::ExprLoop) {
        self.nested += 1;
        visit::visit_expr_loop(self, node);
        self.nested -= 1;
    }
    fn visit_expr_closure(&mut self, node: &'ast syn::ExprClosure) {
        self.nested += 1;
        visit::visit_expr_closure(self, node);
        self.nested -= 1;
    }
    fn visit_expr_async(&mut self, node: &'ast syn::ExprAsync) {
        self.nested += 1;
        visit::visit_expr_async(self, node);
        self.nested -= 1;
    }
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if self.nested > 0
            && let Expr::Path(path) = node.func.as_ref()
            && path
                .path
                .segments
                .last()
                .is_some_and(|s| s.ident.to_string().starts_with("use_"))
        {
            self.error = Some(syn::Error::new_spanned(
                node,
                "hooks must be initialized on the component's unconditional path; extract a child component",
            ));
        }
        visit::visit_expr_call(self, node);
    }
}

struct Nodes(Vec<Node>);
enum Node {
    Text(Expr),
    Group(Nodes),
    Component(Ident, Vec<(Ident, Expr)>),
    If(Expr, Nodes, Nodes),
    Match(Expr, Vec<Arm>),
    List(Ident, Expr, Expr, Nodes),
    KeepAlive(usize, Box<Node>),
}
struct Arm {
    pat: Pat,
    guard: Option<Expr>,
    body: Nodes,
}

impl Parse for Nodes {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut nodes = Vec::new();
        while !input.is_empty() {
            nodes.push(input.parse()?);
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            } else if input.peek(Token![;]) {
                input.parse::<Token![;]>()?;
            }
        }
        Ok(Self(nodes))
    }
}
fn block(input: ParseStream) -> Result<Nodes> {
    let content;
    braced!(content in input);
    content.parse()
}

impl Parse for Node {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.peek(Token![if]) {
            input.parse::<Token![if]>()?;
            let condition = Expr::parse_without_eager_brace(input)?;
            let yes = block(input)?;
            let no = if input.peek(Token![else]) {
                input.parse::<Token![else]>()?;
                if input.peek(Token![if]) {
                    Nodes(vec![input.parse()?])
                } else {
                    block(input)?
                }
            } else {
                Nodes(Vec::new())
            };
            return Ok(Self::If(condition, yes, no));
        }
        if input.peek(Token![match]) {
            input.parse::<Token![match]>()?;
            let source = Expr::parse_without_eager_brace(input)?;
            let content;
            braced!(content in input);
            let mut arms = Vec::new();
            while !content.is_empty() {
                let pat = Pat::parse_multi_with_leading_vert(&content)?;
                let guard = if content.peek(Token![if]) {
                    content.parse::<Token![if]>()?;
                    Some(content.parse()?)
                } else {
                    None
                };
                content.parse::<Token![=>]>()?;
                let body = if content.peek(syn::token::Brace) {
                    block(&content)?
                } else {
                    Nodes(vec![content.parse()?])
                };
                arms.push(Arm { pat, guard, body });
                if content.peek(Token![,]) {
                    content.parse::<Token![,]>()?;
                }
            }
            return Ok(Self::Match(source, arms));
        }
        if input.peek(Token![for]) {
            input.parse::<Token![for]>()?;
            let item = input.parse()?;
            input.parse::<Token![in]>()?;
            let source = Expr::parse_without_eager_brace(input)?;
            if !input.peek(kw::key) {
                return Err(input.error("dynamic lists require an explicit stable key"));
            }
            input.parse::<kw::key>()?;
            let key = Expr::parse_without_eager_brace(input)?;
            return Ok(Self::List(item, source, key, block(input)?));
        }
        let name: Ident = input.parse()?;
        if name == "Column" || name == "Row" {
            return Ok(Self::Group(block(input)?));
        }
        let args;
        parenthesized!(args in input);
        if name == "Text" {
            return Ok(Self::Text(args.parse()?));
        }
        if name == "KeepAlive" {
            let option: Ident = args.parse()?;
            if option != "max_entries" {
                return Err(syn::Error::new_spanned(
                    option,
                    "probe supports max_entries only",
                ));
            }
            args.parse::<Token![:]>()?;
            let limit: syn::LitInt = args.parse()?;
            let nodes = block(input)?;
            if nodes.0.len() != 1 || !matches!(nodes.0[0], Self::If(..) | Self::Match(..)) {
                return Err(syn::Error::new_spanned(
                    name,
                    "KeepAlive requires one direct if or match",
                ));
            }
            return Ok(Self::KeepAlive(
                limit.base10_parse()?,
                Box::new(nodes.0.into_iter().next().unwrap()),
            ));
        }
        let mut props = Vec::new();
        while !args.is_empty() {
            let field = args.parse()?;
            args.parse::<Token![:]>()?;
            props.push((field, args.parse()?));
            if !args.is_empty() {
                args.parse::<Token![,]>()?;
            }
        }
        Ok(Self::Component(name, props))
    }
}

#[proc_macro]
pub fn view(input: TokenStream) -> TokenStream {
    let nodes = parse_macro_input!(input as Nodes);
    match emit_nodes(&nodes) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

fn emit_nodes(nodes: &Nodes) -> Result<Tokens> {
    let emitted = nodes.0.iter().map(emit).collect::<Result<Vec<_>>>()?;
    Ok(quote! { ::probe_ui::View::group(vec![#(#emitted),*]) })
}

#[derive(Default)]
struct Bindings {
    names: BTreeMap<String, Ident>,
    error: Option<syn::Error>,
}
impl<'ast> Visit<'ast> for Bindings {
    fn visit_pat(&mut self, node: &'ast Pat) {
        if matches!(node, Pat::Macro(_)) {
            self.error = Some(syn::Error::new_spanned(
                node,
                "reactive patterns must be visible to view!; expand pattern macros before using them here",
            ));
        }
        visit::visit_pat(self, node);
    }
    fn visit_pat_ident(&mut self, node: &'ast syn::PatIdent) {
        if node.by_ref.is_some() || node.mutability.is_some() {
            self.error = Some(syn::Error::new_spanned(
                node,
                "reactive payload bindings must be owned and immutable (no ref or mut)",
            ));
        }
        self.names
            .insert(node.ident.to_string(), node.ident.clone());
        visit::visit_pat_ident(self, node);
    }
}

impl Bindings {
    fn condition(&mut self, expression: &Expr) {
        match expression {
            Expr::Let(expression) => self.visit_pat(&expression.pat),
            Expr::Binary(expression) if matches!(expression.op, syn::BinOp::And(_)) => {
                self.condition(&expression.left);
                self.condition(&expression.right);
            }
            _ => {}
        }
    }
}

/// Payload values are selected together by native Rust control flow. Field
/// ReadProps project that tuple without rematching the original scrutinee.
fn selection(index: usize, bindings: Bindings, body: &Nodes) -> Result<Tokens> {
    if let Some(error) = bindings.error {
        return Err(error);
    }
    let body = emit_nodes(body)?;
    // syn cannot resolve a bare PatIdent as a binding, constant, or unit
    // variant. Export only names referenced by the view, leaving ordinary
    // unused unit/constant patterns to rustc (notably Option::None).
    let mut used = std::collections::BTreeSet::new();
    collect_identifiers(body.clone(), &mut used);
    let names: Vec<_> = bindings
        .names
        .iter()
        .filter(|(name, _)| used.contains(*name) || used.contains(&format!("r#{name}")))
        .map(|(_, name)| name)
        .collect();
    let payload = format_ident!("__probe_payload", span = proc_macro2::Span::mixed_site());
    // A nested pair has no standard-library tuple arity ceiling for
    // Clone/PartialEq, unlike a flat tuple with more than twelve bindings.
    let value = names
        .iter()
        .rev()
        .fold(quote! { () }, |tail, name| quote! { (#name, #tail) });
    let projections = names.iter().enumerate().map(|(index, name)| {
        let field = (0..index).fold(quote! { value }, |field, _| quote! { #field.1 });
        quote! {
            #[allow(unused_variables)]
            let #name = #payload.project(|value| #field.0.clone());
        }
    });
    Ok(quote! {
        ::probe_ui::Selection::new(#index, #value, move |#payload| {
            #(#projections)*
            #body
        })
    })
}

fn collect_identifiers(tokens: Tokens, names: &mut std::collections::BTreeSet<String>) {
    for token in tokens {
        match token {
            TokenTree::Ident(name) => {
                names.insert(name.to_string());
            }
            TokenTree::Group(group) => collect_identifiers(group.stream(), names),
            TokenTree::Literal(literal) => {
                // Rust format_args! can capture a local without an Ident
                // token, e.g. format!("{name}"). Include identifier-looking
                // words inside placeholders so such captures cannot
                // silently freeze the first payload value.
                if let Ok(literal) = syn::parse_str::<syn::LitStr>(&literal.to_string()) {
                    let value = literal.value();
                    let mut chars = value.chars().peekable();
                    while let Some(character) = chars.next() {
                        if character != '{' {
                            continue;
                        }
                        if chars.peek() == Some(&'{') {
                            chars.next();
                            continue;
                        }
                        let placeholder: String =
                            chars.by_ref().take_while(|c| *c != '}').collect();
                        for word in placeholder.split(|c: char| !c.is_alphanumeric() && c != '_') {
                            if !word.is_empty() {
                                names.insert(word.to_owned());
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn emit(node: &Node) -> Result<Tokens> {
    Ok(match node {
        Node::Text(value) => quote! { ::probe_ui::View::text(move || format!("{}", #value)) },
        Node::Group(nodes) => emit_nodes(nodes)?,
        Node::Component(name, props) => {
            let fields = props.iter().map(|(name, value)| {
                quote! {
                    #name: ::probe_ui::bind_prop(move || #value)
                }
            });
            quote! { #name::mount(#name::Props { #(#fields),* }) }
        }
        Node::If(condition, yes, no) => {
            let mut bindings = Bindings::default();
            bindings.condition(condition);
            let yes = selection(0, bindings, yes)?;
            let no = selection(1, Bindings::default(), no)?;
            quote! { ::probe_ui::View::select(move || {
                if #condition { #yes } else { #no }
            }) }
        }
        Node::Match(source, arms) => {
            let mut selectors = Vec::new();
            for (index, arm) in arms.iter().enumerate() {
                let pat = &arm.pat;
                let guard = arm.guard.as_ref().map(|g| quote! { if #g });
                let mut bindings = Bindings::default();
                bindings.visit_pat(pat);
                let selected = selection(index, bindings, &arm.body)?;
                selectors.push(quote! { #pat #guard => #selected });
            }
            quote! { ::probe_ui::View::select(move || {
                match #source { #(#selectors),* }
            }) }
        }
        Node::List(item, source, key, body) => {
            let body = emit_nodes(body)?;
            quote! { ::probe_ui::View::keyed(
                move || #source,
                |#item| #key,
                move |#item| #body,
            ) }
        }
        Node::KeepAlive(limit, child) => {
            let child = emit(child)?;
            quote! { (#child).keep_alive(#limit) }
        }
    })
}
