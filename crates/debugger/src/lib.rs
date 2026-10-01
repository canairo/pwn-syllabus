//! Debugger algorithms have no GUI dependency and operate on an execution backend.
#![forbid(unsafe_code)]
use vm_core::{Address, Allocation, ExecutionBackend, Memory};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueType {
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
    Pointer,
    Ascii,
    Utf8,
}
impl ValueType {
    pub const ALL: [Self; 13] = [
        Self::U8,
        Self::U16,
        Self::U32,
        Self::U64,
        Self::I8,
        Self::I16,
        Self::I32,
        Self::I64,
        Self::F32,
        Self::F64,
        Self::Pointer,
        Self::Ascii,
        Self::Utf8,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::F32 => "f32",
            Self::F64 => "f64",
            Self::Pointer => "pointer (u64)",
            Self::Ascii => "ASCII",
            Self::Utf8 => "UTF-8",
        }
    }
    pub fn width(self) -> usize {
        match self {
            Self::U8 | Self::I8 => 1,
            Self::U16 | Self::I16 => 2,
            Self::U32 | Self::I32 | Self::F32 => 4,
            Self::Ascii | Self::Utf8 => 16,
            _ => 8,
        }
    }
    pub fn interpret(self, bytes: &[u8]) -> Result<String, String> {
        if bytes.len() < self.width() {
            return Err("not enough readable bytes".into());
        }
        let b = &bytes[..self.width()];
        let bits = unsigned(&b[..b.len().min(8)]);
        Ok(match self {
            Self::U8 | Self::U16 | Self::U32 | Self::U64 => bits.to_string(),
            Self::Pointer => format!("0x{bits:016X}"),
            Self::I8 => (bits as i8).to_string(),
            Self::I16 => (bits as i16).to_string(),
            Self::I32 => (bits as i32).to_string(),
            Self::I64 => (bits as i64).to_string(),
            Self::F32 => f32::from_bits(bits as u32).to_string(),
            Self::F64 => f64::from_bits(bits).to_string(),
            Self::Ascii => b
                .iter()
                .map(|c| {
                    if c.is_ascii_graphic() || *c == b' ' {
                        *c as char
                    } else {
                        '.'
                    }
                })
                .collect(),
            Self::Utf8 => std::str::from_utf8(b)
                .map_err(|_| "selection is not valid UTF-8")?
                .trim_end_matches('\0')
                .into(),
        })
    }
    pub fn encode(self, text: &str) -> Result<Vec<u8>, String> {
        let invalid = || format!("enter a valid {} value", self.label());
        let t = text.trim();
        let bits = match self {
            Self::U8 | Self::U16 | Self::U32 | Self::U64 | Self::Pointer => {
                let v = parse_u64(t)?;
                let max = match self.width() {
                    1 => u8::MAX as u64,
                    2 => u16::MAX as u64,
                    4 => u32::MAX as u64,
                    _ => u64::MAX,
                };
                if v > max {
                    return Err(invalid());
                }
                v
            }
            Self::I8 => t.parse::<i8>().map_err(|_| invalid())? as u64,
            Self::I16 => t.parse::<i16>().map_err(|_| invalid())? as u64,
            Self::I32 => t.parse::<i32>().map_err(|_| invalid())? as u64,
            Self::I64 => t.parse::<i64>().map_err(|_| invalid())? as u64,
            Self::F32 => t.parse::<f32>().map_err(|_| invalid())?.to_bits() as u64,
            Self::F64 => t.parse::<f64>().map_err(|_| invalid())?.to_bits(),
            Self::Ascii | Self::Utf8 => {
                if self == Self::Ascii && !text.is_ascii() {
                    return Err("ASCII accepts only ASCII characters".into());
                }
                return Ok(text.as_bytes().to_vec());
            }
        };
        Ok(bits.to_le_bytes()[..self.width()].to_vec())
    }
}
pub fn parse_u64(text: &str) -> Result<u64, String> {
    let t = text.trim().replace('_', "");
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16)
    } else if let Some(binary) = t.strip_prefix("0b").or_else(|| t.strip_prefix("0B")) {
        u64::from_str_radix(binary, 2)
    } else {
        t.parse()
    }
    .map_err(|_| "enter decimal, 0x hexadecimal, or 0b binary digits".into())
}
pub fn parse_hex(text: &str) -> Result<Vec<u8>, String> {
    let cleaned: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if !cleaned.len().is_multiple_of(2) || !cleaned.is_ascii() {
        return Err("enter pairs of hexadecimal digits, such as 20 31 01 00".into());
    }
    cleaned
        .as_bytes()
        .chunks(2)
        .map(|b| {
            u8::from_str_radix(std::str::from_utf8(b).expect("ASCII checked"), 16)
                .map_err(|_| "invalid hexadecimal byte".into())
        })
        .collect()
}
pub fn unsigned(bytes: &[u8]) -> u64 {
    let mut v = [0; 8];
    let n = bytes.len().min(8);
    v[..n].copy_from_slice(&bytes[..n]);
    u64::from_le_bytes(v)
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanFilter {
    Exact,
    Equal,
    NotEqual,
    Changed,
    Unchanged,
    Increased,
    Decreased,
    IncreasedBy,
    DecreasedBy,
}
impl ScanFilter {
    pub const ALL: [Self; 9] = [
        Self::Exact,
        Self::Equal,
        Self::NotEqual,
        Self::Changed,
        Self::Unchanged,
        Self::Increased,
        Self::Decreased,
        Self::IncreasedBy,
        Self::DecreasedBy,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Exact => "Exact value",
            Self::Equal => "Equal to value",
            Self::NotEqual => "Not equal to value",
            Self::Changed => "Changed",
            Self::Unchanged => "Unchanged",
            Self::Increased => "Increased",
            Self::Decreased => "Decreased",
            Self::IncreasedBy => "Increased by",
            Self::DecreasedBy => "Decreased by",
        }
    }
    pub fn needs_value(self) -> bool {
        matches!(
            self,
            Self::Exact | Self::Equal | Self::NotEqual | Self::IncreasedBy | Self::DecreasedBy
        )
    }
}
#[derive(Debug, Clone)]
pub struct ScanResult {
    pub address: Address,
    pub previous: Vec<u8>,
}
#[derive(Debug, Default)]
pub struct Scanner {
    pub results: Vec<ScanResult>,
    pub width: usize,
    pub initialized: bool,
    pub byte_pattern: bool,
}
impl Scanner {
    pub fn first<B: ExecutionBackend>(
        &mut self,
        backend: &B,
        width: usize,
        exact: Option<u64>,
        aligned: bool,
    ) -> Result<(), String> {
        if ![1, 2, 4, 8].contains(&width) {
            return Err("scan width must be 1, 2, 4, or 8 bytes".into());
        }
        if exact.is_some_and(|v| width < 8 && v > ((1u64 << (width * 8)) - 1)) {
            return Err("value does not fit the selected width".into());
        }
        let pattern = exact.map(|v| v.to_le_bytes()[..width].to_vec());
        self.scan(backend, width, pattern.as_deref(), aligned, false)
    }
    pub fn first_bytes<B: ExecutionBackend>(
        &mut self,
        backend: &B,
        pattern: &[u8],
    ) -> Result<(), String> {
        if pattern.is_empty() || pattern.len() > 256 {
            return Err("text or byte patterns must contain 1 to 256 bytes".into());
        }
        self.scan(backend, pattern.len(), Some(pattern), false, true)
    }
    fn scan<B: ExecutionBackend>(
        &mut self,
        backend: &B,
        width: usize,
        pattern: Option<&[u8]>,
        aligned: bool,
        byte_pattern: bool,
    ) -> Result<(), String> {
        let mut results = vec![];
        for r in backend
            .memory_regions()
            .iter()
            .filter(|r| r.permissions().read && r.permissions().write)
        {
            let bytes = backend
                .read_memory(r.base(), r.len())
                .map_err(|e| e.to_string())?;
            for off in
                (0..bytes.len().saturating_sub(width - 1)).step_by(if aligned { width } else { 1 })
            {
                let address = r.base() + off as u64;
                if aligned && !address.is_multiple_of(width as u64) {
                    continue;
                }
                let value = &bytes[off..off + width];
                if pattern.is_none_or(|p| p == value) {
                    results.push(ScanResult {
                        address,
                        previous: value.to_vec(),
                    });
                }
            }
        }
        self.results = results;
        self.width = width;
        self.initialized = true;
        self.byte_pattern = byte_pattern;
        Ok(())
    }
    pub fn filter<B: ExecutionBackend>(
        &mut self,
        backend: &B,
        filter: ScanFilter,
        target: Option<u64>,
    ) -> Result<(), String> {
        if self.byte_pattern {
            return Err("this scan contains text/bytes; use a byte filter".into());
        }
        if target.is_some_and(|v| self.width < 8 && v > ((1u64 << (self.width * 8)) - 1)) {
            return Err("value does not fit the scan width".into());
        }
        let bytes = target.map(|v| v.to_le_bytes()[..self.width.min(8)].to_vec());
        self.filter_impl(backend, filter, bytes.as_deref())
    }
    pub fn filter_bytes<B: ExecutionBackend>(
        &mut self,
        backend: &B,
        filter: ScanFilter,
        target: Option<&[u8]>,
    ) -> Result<(), String> {
        if !self.byte_pattern {
            return Err("this is a number scan".into());
        }
        if !matches!(
            filter,
            ScanFilter::Exact
                | ScanFilter::Equal
                | ScanFilter::NotEqual
                | ScanFilter::Changed
                | ScanFilter::Unchanged
        ) {
            return Err(
                "text/byte scans support exact, equal, not equal, changed, and unchanged filters"
                    .into(),
            );
        }
        if target.is_some_and(|v| v.len() != self.width) {
            return Err("keep the same byte length when narrowing text/pattern results".into());
        }
        self.filter_impl(backend, filter, target)
    }
    fn filter_impl<B: ExecutionBackend>(
        &mut self,
        backend: &B,
        filter: ScanFilter,
        target: Option<&[u8]>,
    ) -> Result<(), String> {
        if !self.initialized {
            return Err("run a first scan before filtering".into());
        }
        if filter.needs_value() && target.is_none() {
            return Err("enter a filter value".into());
        }
        let amount = target.map(unsigned).unwrap_or(0);
        self.results.retain_mut(|r| {
            let Ok(now) = backend.read_memory(r.address, self.width as u64) else {
                return false;
            };
            let old = unsigned(&r.previous);
            let value = unsigned(&now);
            let keep = match filter {
                ScanFilter::Exact | ScanFilter::Equal => Some(now.as_slice()) == target,
                ScanFilter::NotEqual => Some(now.as_slice()) != target,
                ScanFilter::Changed => now != r.previous,
                ScanFilter::Unchanged => now == r.previous,
                ScanFilter::Increased => value > old,
                ScanFilter::Decreased => value < old,
                ScanFilter::IncreasedBy => value.checked_sub(old) == Some(amount),
                ScanFilter::DecreasedBy => old.checked_sub(value) == Some(amount),
            };
            r.previous = now;
            keep
        });
        Ok(())
    }
}
#[derive(Debug, Clone)]
pub struct Watch {
    pub address: Address,
    pub kind: ValueType,
    pub name: String,
}
impl Watch {
    pub fn value<B: ExecutionBackend>(&self, b: &B) -> Result<String, String> {
        self.kind.interpret(
            &b.read_memory(self.address, self.kind.width() as u64)
                .map_err(|e| e.to_string())?,
        )
    }
}
pub fn pointer_label(address: Address, allocations: &[Allocation]) -> String {
    if let Some(a) = allocations
        .iter()
        .rev()
        .find(|a| a.live && a.contains(address))
    {
        format!("allocation #{} + 0x{:X}", a.id, address - a.base)
    } else {
        "no live allocation".into()
    }
}
#[derive(Debug, Clone)]
pub struct Diff {
    pub address: Address,
    pub before: u8,
    pub after: u8,
}
pub fn diff<B: ExecutionBackend>(snapshot: &Memory, backend: &B) -> Vec<Diff> {
    let mut changed = vec![];
    for old in snapshot.regions() {
        if let Ok(new) = backend.read_memory(old.base(), old.len()) {
            for (i, (&before, &after)) in old.bytes().iter().zip(new.iter()).enumerate() {
                if before != after {
                    changed.push(Diff {
                        address: old.base() + i as u64,
                        before,
                        after,
                    });
                }
            }
        }
    }
    changed
}
#[cfg(test)]
mod tests {
    use super::*;
    pub(super) struct Backend {
        pub(super) memory: Memory,
    }
    impl ExecutionBackend for Backend {
        type Snapshot = Memory;
        fn reset(&mut self, _: u64) -> Result<(), String> {
            Ok(())
        }
        fn step(&mut self) -> Result<(), String> {
            Ok(())
        }
        fn set_paused(&mut self, _: bool) {}
        fn is_paused(&self) -> bool {
            false
        }
        fn read_memory(&self, a: Address, n: u64) -> vm_core::Result<Vec<u8>> {
            self.memory.read(a, n)
        }
        fn write_memory(&mut self, a: Address, b: &[u8]) -> Result<(), String> {
            self.memory
                .write(a, b, vm_core::WriteSource::Debugger, 0, "test")
                .map_err(|e| e.to_string())
        }
        fn memory_regions(&self) -> &[vm_core::Region] {
            self.memory.regions()
        }
        fn allocations(&self) -> &[Allocation] {
            &[]
        }
        fn snapshot(&self) -> Memory {
            self.memory.snapshot()
        }
        fn restore(&mut self, s: Memory) -> Result<(), String> {
            self.memory.restore(s).map_err(|e| e.to_string())
        }
    }
    fn backend() -> Backend {
        let mut memory = Memory::default();
        memory
            .map_region(
                0x555500000000,
                64,
                vm_core::Permissions::RW,
                vm_core::RegionKind::Heap,
            )
            .unwrap();
        Backend { memory }
    }
    #[test]
    fn scan_narrowing_and_unknown_filters() {
        let mut b = backend();
        let a = b.memory.regions()[0].base();
        b.write_memory(a, &127u64.to_le_bytes()).unwrap();
        b.write_memory(a + 8, &127u64.to_le_bytes()).unwrap();
        let mut s = Scanner::default();
        s.first(&b, 8, Some(127), true).unwrap();
        assert_eq!(s.results.len(), 2);
        b.write_memory(a, &128u64.to_le_bytes()).unwrap();
        s.filter(&b, ScanFilter::Exact, Some(128)).unwrap();
        assert_eq!(s.results[0].address, a);
        s.first(&b, 8, None, true).unwrap();
        b.write_memory(a, &129u64.to_le_bytes()).unwrap();
        s.filter(&b, ScanFilter::IncreasedBy, Some(1)).unwrap();
        assert_eq!(s.results.len(), 1);
        assert_eq!(s.results[0].address, a);
    }
    #[test]
    fn typed_values_and_diff() {
        assert_eq!(
            ValueType::U64.encode("0x555500001070").unwrap(),
            0x555500001070u64.to_le_bytes()
        );
        assert!(ValueType::U8.encode("256").is_err());
        assert_eq!(ValueType::I16.interpret(&[255, 255]).unwrap(), "-1");
        assert_eq!(parse_hex("20 31 01 00").unwrap(), vec![32, 49, 1, 0]);
        assert!(parse_hex("0xQQ").is_err());
        let mut b = backend();
        let snap = b.snapshot();
        let a = b.memory.regions()[0].base();
        b.write_memory(a, &[42]).unwrap();
        assert_eq!(diff(&snap, &b).len(), 1);
    }
}

#[cfg(test)]
mod pattern_tests {
    use super::*;
    #[test]
    fn all_number_bases_and_utf8_patterns() {
        assert_eq!(parse_u64("127").unwrap(), 127);
        assert_eq!(parse_u64("0x7f").unwrap(), 127);
        assert_eq!(parse_u64("0b111_1111").unwrap(), 127);
        assert!(parse_u64("0b2").is_err());
        let mut memory = Memory::default();
        memory
            .map_region(
                0x555500000000,
                64,
                vm_core::Permissions::RW,
                vm_core::RegionKind::Heap,
            )
            .unwrap();
        let mut b = super::tests::Backend { memory };
        let base = b.memory.regions()[0].base();
        b.write_memory(base + 3, "饼干".as_bytes()).unwrap();
        b.write_memory(base + 20, "饼干".as_bytes()).unwrap();
        let mut scanner = Scanner::default();
        scanner.first_bytes(&b, "饼干".as_bytes()).unwrap();
        assert_eq!(scanner.results.len(), 2);
        b.write_memory(base + 20, "饼果".as_bytes()).unwrap();
        scanner
            .filter_bytes(&b, ScanFilter::Unchanged, None)
            .unwrap();
        assert_eq!(scanner.results[0].address, base + 3);
        scanner
            .first_bytes(&b, &parse_hex("E9 A5 BC E5 B9 B2").unwrap())
            .unwrap();
        assert_eq!(scanner.results.len(), 1);
        assert!(scanner.first_bytes(&b, &[]).is_err());
        assert!(
            scanner
                .filter_bytes(&b, ScanFilter::Increased, None)
                .is_err()
        );
    }
}
