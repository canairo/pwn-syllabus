//! Deterministic first-fit allocation with out-of-band metadata.
#![forbid(unsafe_code)]
use serde::{Deserialize, Serialize};
use thiserror::Error;
use vm_core::{Address, Allocation};
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum HeapError {
    #[error("invalid allocation size or alignment")]
    InvalidLayout,
    #[error("guest heap exhausted")]
    Exhausted,
    #[error("invalid free at 0x{0:016X}")]
    InvalidFree(Address),
    #[error("double free at 0x{0:016X}")]
    DoubleFree(Address),
    #[error("invalid heap snapshot: {0}")]
    InvalidSnapshot(String),
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FreeBlock {
    pub base: Address,
    pub size: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Heap {
    base: Address,
    size: u64,
    next_id: u64,
    allocations: Vec<Allocation>,
    free: Vec<FreeBlock>,
}
impl Heap {
    pub fn new(base: Address, size: u64) -> Result<Self, HeapError> {
        if base == 0 || size == 0 || base.checked_add(size).is_none() {
            return Err(HeapError::InvalidLayout);
        }
        Ok(Self {
            base,
            size,
            next_id: 1,
            allocations: vec![],
            free: vec![FreeBlock { base, size }],
        })
    }
    pub fn base(&self) -> Address {
        self.base
    }
    pub fn size(&self) -> u64 {
        self.size
    }
    pub fn allocations(&self) -> &[Allocation] {
        &self.allocations
    }
    pub fn free_list(&self) -> &[FreeBlock] {
        &self.free
    }
    pub fn allocation_for(&self, a: Address) -> Option<&Allocation> {
        self.allocations
            .iter()
            .rev()
            .find(|x| x.live && x.contains(a))
    }
    pub fn allocate(
        &mut self,
        size: u64,
        alignment: u64,
        semantic_type: &str,
    ) -> Result<Address, HeapError> {
        if size == 0 || !alignment.is_power_of_two() {
            return Err(HeapError::InvalidLayout);
        }
        if self.allocations.len() >= 65536 || self.next_id == u64::MAX {
            return Err(HeapError::Exhausted);
        }
        for i in 0..self.free.len() {
            let b = &self.free[i];
            let Some(aligned) = b
                .base
                .checked_add(alignment - 1)
                .map(|v| v & !(alignment - 1))
            else {
                continue;
            };
            let Some(end) = aligned.checked_add(size) else {
                continue;
            };
            let b_end = b.base + b.size;
            if end > b_end {
                continue;
            }
            let generation = self
                .allocations
                .iter()
                .filter(|a| a.base == aligned)
                .map(|a| a.generation)
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or(HeapError::Exhausted)?;
            let old = self.free.remove(i);
            let mut replacement = vec![];
            if aligned > old.base {
                replacement.push(FreeBlock {
                    base: old.base,
                    size: aligned - old.base,
                });
            }
            if end < b_end {
                replacement.push(FreeBlock {
                    base: end,
                    size: b_end - end,
                });
            }
            self.free.splice(i..i, replacement);
            self.allocations.push(Allocation {
                id: self.next_id,
                base: aligned,
                size,
                alignment,
                live: true,
                generation,
                semantic_type: semantic_type.into(),
            });
            self.next_id += 1;
            return Ok(aligned);
        }
        Err(HeapError::Exhausted)
    }
    pub fn free(&mut self, address: Address) -> Result<(), HeapError> {
        let Some(a) = self
            .allocations
            .iter_mut()
            .rev()
            .find(|a| a.base == address && a.live)
        else {
            return Err(if self.allocations.iter().any(|a| a.base == address) {
                HeapError::DoubleFree(address)
            } else {
                HeapError::InvalidFree(address)
            });
        };
        a.live = false;
        self.free.push(FreeBlock {
            base: a.base,
            size: a.size,
        });
        self.free.sort_by_key(|b| b.base);
        let mut merged: Vec<FreeBlock> = vec![];
        for b in self.free.drain(..) {
            if let Some(prev) = merged.last_mut().filter(|p| p.base + p.size == b.base) {
                prev.size += b.size;
            } else {
                merged.push(b);
            }
        }
        self.free = merged;
        Ok(())
    }
    pub fn describe_write(
        &self,
        address: Address,
        length: u64,
        origin_address: Option<Address>,
    ) -> (Option<u64>, Vec<u64>, u64) {
        let end = address.saturating_add(length);
        let origin = origin_address.and_then(|a| self.allocation_for(a));
        let affected = self
            .allocations
            .iter()
            .filter(|a| a.live && address < a.end() && end > a.base)
            .map(|a| a.id)
            .collect();
        let overflow = origin.map(|a| end.saturating_sub(a.end())).unwrap_or(0);
        (origin.map(|a| a.id), affected, overflow)
    }
    pub fn validate(&self) -> Result<(), HeapError> {
        let invalid = |s: &str| HeapError::InvalidSnapshot(s.into());
        let end = self
            .base
            .checked_add(self.size)
            .ok_or_else(|| invalid("address overflow"))?;
        if self.base == 0 || self.size == 0 || self.allocations.len() > 65536 {
            return Err(invalid("invalid bounds"));
        }
        let mut ids = std::collections::HashSet::new();
        let mut spans = vec![];
        for a in &self.allocations {
            if !ids.insert(a.id)
                || a.id == 0
                || a.id >= self.next_id
                || a.generation == 0
                || a.size == 0
                || !a.alignment.is_power_of_two()
                || !a.base.is_multiple_of(a.alignment)
                || a.base < self.base
                || a.base.checked_add(a.size).is_none_or(|e| e > end)
            {
                return Err(invalid("allocation metadata"));
            }
            if a.live {
                spans.push((a.base, a.end()));
            }
        }
        let mut prev = 0;
        for b in &self.free {
            if b.size == 0
                || b.base < self.base
                || b.base < prev
                || b.base.checked_add(b.size).is_none_or(|e| e > end)
            {
                return Err(invalid("free list"));
            }
            prev = b.base + b.size;
            spans.push((b.base, prev));
        }
        spans.sort_unstable();
        let mut cursor = self.base;
        for (start, end) in spans {
            if start != cursor {
                return Err(invalid("overlapping or missing heap spans"));
            }
            cursor = end;
        }
        if cursor != end {
            return Err(invalid("heap coverage"));
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reuse_and_coalescing() {
        let mut h = Heap::new(0x555500000000, 1024).unwrap();
        let a = h.allocate(32, 8, "Name").unwrap();
        let b = h.allocate(64, 16, "Stats").unwrap();
        assert_eq!(b, a + 32);
        assert_eq!(h.allocation_for(b + 63).unwrap().base, b);
        h.free(a).unwrap();
        let c = h.allocate(16, 8, "Effect").unwrap();
        assert_eq!(a, c);
        assert_eq!(h.allocation_for(c).unwrap().generation, 2);
        assert!(h.free(c + 1).is_err());
        h.free(c).unwrap();
        assert!(matches!(h.free(c), Err(HeapError::DoubleFree(_))));
        h.free(b).unwrap();
        assert_eq!(
            h.free_list(),
            &[FreeBlock {
                base: a,
                size: 1024
            }]
        );
        h.validate().unwrap();
    }
    #[test]
    fn allocator_sequences() {
        for seed in 0..128u64 {
            let mut h = Heap::new(0x555500000000, 4096).unwrap();
            let mut rng = seed + 1;
            let mut live = vec![];
            for _ in 0..200 {
                rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
                if rng.is_multiple_of(3) && !live.is_empty() {
                    let a = live.remove(rng as usize % live.len());
                    h.free(a).unwrap();
                } else if let Ok(a) = h.allocate((rng % 96) + 1, 1 << (rng % 6), "object") {
                    live.push(a);
                }
                h.validate().unwrap();
            }
        }
    }
}
