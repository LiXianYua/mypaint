//! Tile operation queue + dirty tile tracking.
//! 对应 operationqueue.c + tilemap.c。

use crate::render::mask::Premul15;
use std::collections::HashMap;

/// 一个 dab 操作，已经准备好被某个 tile 渲染。
/// 对应 OperationDataDrawDab in operationqueue.c。
#[derive(Debug, Clone, Copy)]
pub struct OpDrawDab {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub color_r: Premul15,
    pub color_g: Premul15,
    pub color_b: Premul15,
    pub color_a: f32, // 0..1
    pub opaque: f32,
    pub hardness: f32,
    pub softness: f32,
    pub aspect_ratio: f32,
    pub angle: f32,
    pub lock_alpha: f32,
    pub colorize: f32,
    pub posterize: f32,
    pub posterize_num: u16,
    pub paint: f32,
    pub normal: f32, // = 1 * (1-lock_alpha) * (1-colorize) * (1-posterize)
}

/// Tile 坐标（按 tile 单位，不是像素）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileIndex {
    pub x: i32,
    pub y: i32,
}

/// 按 tile 索引存储待处理 op，跟踪 dirty tiles。
pub struct OperationQueue {
    queues: HashMap<TileIndex, Vec<OpDrawDab>>,
}

impl OperationQueue {
    pub fn new() -> Self {
        Self {
            queues: HashMap::new(),
        }
    }

    /// 推入 op 到指定 tile 的队列。
    pub fn add(&mut self, tile: TileIndex, op: OpDrawDab) {
        self.queues.entry(tile).or_default().push(op);
    }

    /// 取出该 tile 的所有 op（清空该队列）。
    pub fn pop_all(&mut self, tile: TileIndex) -> Vec<OpDrawDab> {
        self.queues.remove(&tile).unwrap_or_default()
    }

    /// 返回所有有 op 的 tile 索引迭代器。
    pub fn dirty_tiles(&self) -> impl Iterator<Item = TileIndex> + '_ {
        self.queues.keys().copied()
    }

    /// 清空所有 dirty tile 标记（process 完毕后调用）。
    pub fn clear_dirty_tiles(&mut self) {
        self.queues.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.queues.is_empty()
    }
    pub fn len(&self) -> usize {
        self.queues.len()
    }
}

impl Default for OperationQueue {
    fn default() -> Self {
        Self::new()
    }
}
