// 構文木を辿ってプロプロセッサ命令に関する情報を集める。

use super::{a_scope::*, a_symbol::*, name_system::*};
use crate::{
    parse::*,
    source::{DocId, Loc},
    token::TokenKind,
    utils::rc_str::RcStr,
};
use std::{collections::HashMap, mem::replace, rc::Rc};

pub(crate) struct ASignatureData {
    pub(crate) name: RcStr,
    pub(crate) params: Vec<(Option<PParamTy>, Option<RcStr>, Option<String>)>,
}

#[derive(Default)]
struct Ctx {
    doc: DocId,
    symbols: Vec<ASymbol>,
    includes: Vec<(RcStr, Loc)>,
    scope: ALocalScope,
    modules: HashMap<AModule, AModuleData>,
    deffuncs: HashMap<ADefFunc, ADefFuncData>,
    module_len: usize,
    deffunc_len: usize,
}

impl Ctx {
    fn privacy_scope_or_local(&self, privacy_opt: &Option<(PPrivacy, PToken)>) -> ADefScope {
        match privacy_opt {
            Some((PPrivacy::Global, _)) => ADefScope::Global,
            _ => ADefScope::Local,
        }
    }

    fn privacy_scope_or_global(&self, privacy_opt: &Option<(PPrivacy, PToken)>) -> ADefScope {
        match privacy_opt {
            Some((PPrivacy::Local, _)) => ADefScope::Local,
            _ => ADefScope::Global,
        }
    }

    fn add_symbol(
        &mut self,
        kind: ASymbolKind,
        leader: &PToken,
        name: &PToken,
        def: ADefScope,
    ) -> ASymbol {
        add_symbol(kind, leader, name, def, &self.scope, &mut self.symbols)
    }
}

fn add_symbol(
    kind: ASymbolKind,
    // 構文ノードの先頭のトークン
    leader: &PToken,
    name: &PToken,
    def: ADefScope,
    local: &ALocalScope,
    symbols: &mut Vec<ASymbol>,
) -> ASymbol {
    let NameScopeNsTriple {
        basename,
        scope_opt,
        ns_opt,
    } = resolve_name_scope_ns_for_def(&name.body.text, def, local);

    let symbol = ASymbol::from(ASymbolData {
        kind,
        name: basename,
        leader_opt: Some(leader.clone()),
        scope_opt,
        ns_opt,

        details_opt: None,
        def_sites: Default::default(),
        use_sites: Default::default(),
        signature_opt: Default::default(),
    });
    symbol.def_sites.borrow_mut().push(name.body.loc);
    symbols.push(symbol.clone());
    symbol
}

fn on_stmt(stmt: &PStmt, ctx: &mut Ctx) {
    match stmt {
        PStmt::Label(PLabel { star, name_opt }) => {
            if let Some(name) = name_opt {
                ctx.add_symbol(ASymbolKind::Label, star, name, ADefScope::Local);
            }
        }
        PStmt::Assign(_) | PStmt::Command(_) | PStmt::Invoke(_) => {}
        PStmt::Const(PConstStmt {
            hash,
            privacy_opt,
            name_opt,
            ..
        }) => {
            if let Some(name) = name_opt {
                let scope = ctx.privacy_scope_or_local(privacy_opt);
                ctx.add_symbol(ASymbolKind::Const, hash, name, scope);
            }
        }
        PStmt::Define(PDefineStmt {
            hash,
            privacy_opt,
            ctype_opt,
            name_opt,
            ..
        }) => {
            if let Some(name) = name_opt {
                let scope = ctx.privacy_scope_or_local(privacy_opt);
                let ctype = ctype_opt.is_some();
                ctx.add_symbol(ASymbolKind::Macro { ctype }, hash, name, scope);
            }
        }
        PStmt::Enum(PEnumStmt {
            hash,
            privacy_opt,
            name_opt,
            ..
        }) => {
            if let Some(name) = name_opt {
                let scope = ctx.privacy_scope_or_local(privacy_opt);
                ctx.add_symbol(ASymbolKind::Enum, hash, name, scope);
            }
        }
        PStmt::DefFunc(stmt) => {
            let PDefFuncStmt {
                hash,
                keyword: _,
                kind,
                privacy_opt,
                name_opt,
                onexit_opt,
                params,
                stmts,
                behind,
                ..
            } = stmt;

            ctx.deffunc_len += 1;
            let deffunc = ADefFunc::new(ctx.deffunc_len);
            ctx.deffuncs.insert(
                deffunc,
                ADefFuncData {
                    content_loc: hash.body.loc.unite(behind),
                },
            );

            let mut symbol_opt = None;

            let kind = match *kind {
                PDefFuncKind::DefFunc => ASymbolKind::DefFunc,
                PDefFuncKind::DefCFunc => ASymbolKind::DefCFunc,
                PDefFuncKind::ModInit | PDefFuncKind::ModTerm | PDefFuncKind::ModFunc => {
                    ASymbolKind::ModFunc
                }
                PDefFuncKind::ModCFunc => ASymbolKind::ModCFunc,
            };

            if let Some(name) = name_opt {
                if onexit_opt.is_none() {
                    let scope = ctx.privacy_scope_or_global(privacy_opt);
                    symbol_opt = Some(ctx.add_symbol(kind, hash, name, scope));
                }
            }

            if let Some(symbol) = symbol_opt {
                if let Some(data) = new_signature_data_for_deffunc(stmt) {
                    *symbol.signature_opt.borrow_mut() = Some(Rc::new(data));
                }
            }

            let parent_deffunc = replace(&mut ctx.scope.deffunc_opt, Some(deffunc));

            for param in params {
                if let Some(name) = &param.name_opt {
                    let param_ty = param.param_ty_opt.as_ref().map(|&(t, _)| t);
                    ctx.add_symbol(ASymbolKind::Param(param_ty), hash, name, ADefScope::Param);
                }
            }

            for stmt in stmts {
                on_stmt(stmt, ctx);
            }

            ctx.scope.deffunc_opt = parent_deffunc;
        }
        PStmt::UseLib(_) => {}
        PStmt::LibFunc(stmt) => {
            let PLibFuncStmt {
                hash,
                privacy_opt,
                name_opt,
                onexit_opt,
                ..
            } = stmt;
            let mut symbol_opt = None;

            if let Some(name) = name_opt {
                if onexit_opt.is_none() {
                    let scope = ctx.privacy_scope_or_local(privacy_opt);
                    symbol_opt = Some(ctx.add_symbol(ASymbolKind::LibFunc, hash, name, scope));
                }
            }

            if let Some(symbol) = symbol_opt {
                if let Some(signature_data) = new_signature_data_for_lib_func(stmt) {
                    *symbol.signature_opt.borrow_mut() = Some(Rc::new(signature_data));
                }
            }
        }
        PStmt::UseCom(PUseComStmt {
            hash,
            privacy_opt,
            name_opt,
            ..
        }) => {
            if let Some(name) = name_opt {
                let scope = ctx.privacy_scope_or_local(privacy_opt);
                ctx.add_symbol(ASymbolKind::ComInterface, hash, name, scope);
            }
        }
        PStmt::ComFunc(PComFuncStmt {
            hash,
            privacy_opt,
            name_opt,
            ..
        }) => {
            if let Some(name) = name_opt {
                let scope = ctx.privacy_scope_or_global(privacy_opt);
                ctx.add_symbol(ASymbolKind::ComFunc, hash, name, scope);
            }
        }
        PStmt::RegCmd(_) => {}
        PStmt::Cmd(PCmdStmt {
            hash,
            privacy_opt,
            name_opt,
            ..
        }) => {
            if let Some(name) = name_opt {
                let scope = ctx.privacy_scope_or_local(privacy_opt);
                ctx.add_symbol(ASymbolKind::PluginCmd, hash, name, scope);
            }
        }
        PStmt::Module(PModuleStmt {
            hash,
            keyword,
            name_opt,
            fields,
            stmts,
            behind,
            ..
        }) => {
            let module = AModule::new(ctx.doc, &mut ctx.module_len, name_opt);

            ctx.modules.insert(
                module.clone(),
                AModuleData {
                    keyword_loc: keyword.body.loc.clone(),
                    content_loc: hash.body.loc.unite(&behind),
                },
            );

            let parent_scope = replace(
                &mut ctx.scope,
                ALocalScope {
                    module_opt: Some(module),
                    deffunc_opt: None,
                },
            );

            if let Some(name) = name_opt {
                ctx.add_symbol(ASymbolKind::Module, hash, name, ADefScope::Global);
            }

            for field in fields.iter().filter_map(|param| param.name_opt.as_ref()) {
                ctx.add_symbol(ASymbolKind::Field, field, field, ADefScope::Local);
            }

            for stmt in stmts {
                on_stmt(stmt, ctx);
            }

            ctx.scope = parent_scope;
        }
        PStmt::Global(_) => {}
        PStmt::Include(stmt) => {
            if let Some(file_path) = &stmt.file_path_opt {
                if file_path.body.kind == TokenKind::Str {
                    let mut text = file_path.body.text.clone();

                    // クオートを外す。
                    let l = if text.starts_with("\"") { 1 } else { 0 };
                    let r = text.len() - (if text.ends_with("\"") { 1 } else { 0 });
                    text = text.slice(l, r);

                    // 標準化する。
                    text = text.replace("\\\\", "/").to_ascii_lowercase().into();

                    let loc = stmt.hash.body.loc.unite(&file_path.behind());
                    ctx.includes.push((text, loc));
                }
            }
        }
        PStmt::UnknownPreProc(_) => {}
    }
}

fn new_signature_data_for_lib_func(stmt: &PLibFuncStmt) -> Option<ASignatureData> {
    let name = stmt.name_opt.as_ref()?.body.text.clone();

    let params = stmt
        .params
        .iter()
        .filter_map(|param| {
            let ty_opt = match param.param_ty_opt {
                Some((ty, _)) if !ty.take_arg() => return None,
                Some((ty, _)) => Some(ty),
                _ => None,
            };
            let name_opt = param.name_opt.as_ref().map(|name| name.body.text.clone());
            Some((ty_opt, name_opt, None))
        })
        .collect::<Vec<_>>();

    Some(ASignatureData { name, params })
}

fn new_signature_data_for_deffunc(stmt: &PDefFuncStmt) -> Option<ASignatureData> {
    let take_modvar = match stmt.kind {
        PDefFuncKind::DefFunc | PDefFuncKind::DefCFunc => false,
        PDefFuncKind::ModFunc | PDefFuncKind::ModCFunc => true,
        PDefFuncKind::ModInit | PDefFuncKind::ModTerm => return None,
    };

    let name = stmt.name_opt.as_ref()?.body.text.clone();

    let mut params = vec![];

    if take_modvar {
        params.push((Some(PParamTy::Modvar), Some("thismod".into()), None));
    }

    for param in &stmt.params {
        let ty_opt = match param.param_ty_opt {
            Some((ty, _)) if !ty.take_arg() => continue,
            Some((ty, _)) => Some(ty),
            _ => None,
        };
        let name_opt = param.name_opt.as_ref().map(|name| name.body.text.clone());

        params.push((ty_opt, name_opt, None));
    }

    Some(ASignatureData { name, params })
}

pub(crate) struct PreprocAnalysisResult {
    pub(crate) symbols: Vec<ASymbol>,
    pub(crate) includes: Vec<(RcStr, Loc)>,
    pub(crate) modules: HashMap<AModule, AModuleData>,
    pub(crate) deffuncs: HashMap<ADefFunc, ADefFuncData>,
}

pub(crate) fn analyze_preproc(doc: DocId, root: &PRoot) -> PreprocAnalysisResult {
    let mut ctx = Ctx::default();
    ctx.doc = doc;

    for stmt in &root.stmts {
        on_stmt(stmt, &mut ctx);
    }

    let Ctx {
        symbols,
        includes,
        modules,
        deffuncs,
        ..
    } = ctx;

    PreprocAnalysisResult {
        symbols,
        includes,
        modules,
        deffuncs,
    }
}
