use brak_ir_hir::hir::{HirBinOp, HirType, HirUnOp};

use crate::mir::{MirBinOp, MirType, MirUnOp};

pub(super) fn lower_hir_type(ty: &HirType) -> MirType {
    match ty {
        HirType::I32 => MirType::I32,
        HirType::I64 => MirType::I64,
        HirType::F32 => MirType::F32,
        HirType::F64 => MirType::F64,
        HirType::Bool => MirType::Bool,
        HirType::String => MirType::String,
        HirType::Void => MirType::Void,
        HirType::Named(s) => MirType::Named(s.clone()),
        HirType::Ptr(t) => MirType::Named(format!("*{}", lower_hir_type(t))),
        HirType::Ref(t) => MirType::Named(format!("&{}", lower_hir_type(t))),
        HirType::Array(t, n) => MirType::Named(format!("[{}; {}]", lower_hir_type(t), n)),
        HirType::Slice(t) => MirType::Named(format!("[{}]", lower_hir_type(t))),
        HirType::Fn(args, ret) => {
            let args_str: Vec<String> =
                args.iter().map(|a| lower_hir_type(a).to_string()).collect();
            MirType::Named(format!(
                "fn({}) -> {}",
                args_str.join(", "),
                lower_hir_type(ret)
            ))
        }
    }
}

pub(super) fn lower_mir_binop(op: HirBinOp) -> Option<MirBinOp> {
    match op {
        HirBinOp::Add => Some(MirBinOp::Add),
        HirBinOp::Sub => Some(MirBinOp::Sub),
        HirBinOp::Mul => Some(MirBinOp::Mul),
        HirBinOp::Div => Some(MirBinOp::Div),
        HirBinOp::Mod => Some(MirBinOp::Mod),
        HirBinOp::Eq => Some(MirBinOp::Eq),
        HirBinOp::Ne => Some(MirBinOp::Ne),
        HirBinOp::Lt => Some(MirBinOp::Lt),
        HirBinOp::Le => Some(MirBinOp::Le),
        HirBinOp::Gt => Some(MirBinOp::Gt),
        HirBinOp::Ge => Some(MirBinOp::Ge),
        HirBinOp::And => Some(MirBinOp::And),
        HirBinOp::Or => Some(MirBinOp::Or),
        HirBinOp::BitAnd => Some(MirBinOp::BitAnd),
        HirBinOp::BitOr => Some(MirBinOp::BitOr),
        HirBinOp::BitXor => Some(MirBinOp::BitXor),
        HirBinOp::Shl => Some(MirBinOp::Shl),
        HirBinOp::Shr => Some(MirBinOp::Shr),
        // Range expressions are lowered by the dedicated for-loop path.
        HirBinOp::Range => None,
    }
}

pub(super) fn lower_mir_unop(op: HirUnOp) -> MirUnOp {
    match op {
        HirUnOp::Neg => MirUnOp::Neg,
        HirUnOp::Not => MirUnOp::Not,
        HirUnOp::BitNot => MirUnOp::BitNot,
    }
}
