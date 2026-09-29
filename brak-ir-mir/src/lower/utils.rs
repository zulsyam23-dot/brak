use crate::mir::{MirBlock, MirTerminator};

pub(super) fn remap_block_ids(blocks: &mut [MirBlock], offset: usize) {
    for block in blocks.iter_mut() {
        match &mut block.terminator {
            MirTerminator::Jump { target, .. } => *target += offset,
            MirTerminator::Branch { then, else_, .. } => {
                *then += offset;
                *else_ += offset;
            }
            MirTerminator::Return { .. } | MirTerminator::Unreachable => {}
        }
    }
}
