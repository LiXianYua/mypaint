//! Tile operation queue + tilemap + FIFO.
//! Corresponds to operationqueue.c, tilemap.c, fifo.c.

use std::collections::HashMap;
use crate::util::rect::Rect;

/// A tile operation (render dab, get color, etc.)
pub struct TileOp {
    pub tx: i32,
    pub ty: i32,
    pub level: i32,
    pub readonly: bool,
}

impl TileOp {
    pub fn new(tx: i32, ty: i32, level: i32, readonly: bool) -> Self {
        Self { tx, ty, level, readonly }
    }
}

/// Simple FIFO-based operation queue.
pub struct OperationQueue {
    pending: Vec<TileOp>,
}

impl OperationQueue {
    pub fn new() -> Self {
        Self { pending: Vec::new() }
    }

    pub fn enqueue(&mut self, op: TileOp) {
        self.pending.push(op);
    }

    pub fn dequeue(&mut self) -> Option<TileOp> {
        if self.pending.is_empty() {
            None
        } else {
            Some(self.pending.remove(0))
        }
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }
}

impl Default for OperationQueue {
    fn default() -> Self { Self::new() }
}

/// Tile storage map. Corresponds to tilemap.c.
pub struct TileMap<T> {
    tiles: HashMap<(i32, i32, i32), T>,
}

impl<T> TileMap<T> {
    pub fn new() -> Self {
        Self { tiles: HashMap::new() }
    }

    pub fn get(&self, tx: i32, ty: i32, level: i32) -> Option<&T> {
        self.tiles.get(&(tx, ty, level))
    }

    pub fn get_mut(&mut self, tx: i32, ty: i32, level: i32) -> Option<&mut T> {
        self.tiles.get_mut(&(tx, ty, level))
    }

    pub fn insert(&mut self, tx: i32, ty: i32, level: i32, tile: T) {
        self.tiles.insert((tx, ty, level), tile);
    }

    pub fn remove(&mut self, tx: i32, ty: i32, level: i32) -> Option<T> {
        self.tiles.remove(&(tx, ty, level))
    }

    pub fn contains_key(&self, tx: i32, ty: i32, level: i32) -> bool {
        self.tiles.contains_key(&(tx, ty, level))
    }
}

impl<T> Default for TileMap<T> {
    fn default() -> Self { Self::new() }
}
