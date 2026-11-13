//! PDO arena 同步（M3-W2 子项 1，B44）：实时环 → 插件 worker 的无锁快照。
//!
//! # 为什么需要它
//!
//! `AGENTS.md` §1 约束：插件**永不进入实时环**，PDO 数据契约放在核心持有的 arena。
//! B43 的 WASM 宿主 spike 里 arena 只是普通 `Vec`（仅供非实时测试夹具），若直接让
//! 实时线程写、插件线程读，会出现**撕裂**（读到一半新一半旧的 SM2/SM3 组合）。
//!
//! 本模块用 **双缓冲 + seqlock（sequence + generation）** 解决：
//!
//! - 写者（实时环，单线程）只写**非活动槽**，写完后原子发布活动槽并让
//!   `generation` 前进；中途序列号为奇数，读者据此退避重试；
//! - 读者（插件 worker，任意线程）读前后各取一次序列号，只有前后一致且为偶数
//!   才接受快照，因此**永远读不到撕裂的 SM2/SM3**；
//! - 读者不持锁，写者不等待读者 → 实时环零阻塞（写者侧只做一次 CAS + 两次
//!   序列号自增 + 定长拷贝）。
//!
//! 数据用 [`AtomicU8`] 承载，避免 `unsafe` 共享可变内存：本模块**不含 `unsafe`**。
//! 代价是每字节一次 relaxed 原子访问；对 SM2/SM3（几十字节）可忽略。
//!
//! # 边界（W2 其余子项不在本模块）
//!
//! - 只保证"快照不撕裂"；不保证写者不覆盖读者尚未读完的旧槽（读者读到旧
//!   generation 是允许的，调用方可比较 [`ArenaSnapshot::generation`] 判断陈旧）；
//! - 单写者契约由 `publishing` 标志强制执行：并发写返回 [`ArenaError::ConcurrentWriter`]，
//!   不破坏序列号；
//! - 插件线程隔离 / fuel / 超时 / 内存上限仍属 W2 后续子项，见
//!   `docs/plan/M3-scope.md`。

use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering};

/// 单方向 PDO 快照的默认容量（字节）。
pub const DEFAULT_PDO_CAPACITY: usize = 64;

/// 容量上限（对应 ESC SM `length` 字段为 u16）。
pub const MAX_PDO_CAPACITY: usize = 65_535;

/// PDO 方向，用于 [`ArenaError::TooLarge`] 指明是哪个缓冲区超限。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Frame {
    /// SM2 / 输出（主站 → 从站）方向。
    Sm2,
    /// SM3 / 输入（从站 → 主站）方向。
    Sm3,
}

impl fmt::Display for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Sm2 => "SM2",
            Self::Sm3 => "SM3",
        })
    }
}

/// arena 操作错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArenaError {
    /// 构造时某个方向容量为 0。
    ZeroCapacity,
    /// 构造时容量超过 [`MAX_PDO_CAPACITY`]。
    CapacityTooLarge { capacity: usize, max: usize },
    /// 发布的数据超过该方向容量（写者侧预检，`publishing` 标志未被占用）。
    TooLarge {
        frame: Frame,
        len: usize,
        capacity: usize,
    },
    /// 另一个写者正在发布（单写者契约被违反）。
    ConcurrentWriter,
    /// 读者在 `retries` 次重试后仍未拿到稳定的快照（写者发布过于频繁）。
    Contended { retries: u32 },
}

impl fmt::Display for ArenaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroCapacity => f.write_str("PDO arena capacity must be non-zero"),
            Self::CapacityTooLarge { capacity, max } => {
                write!(f, "PDO arena capacity {capacity} exceeds max {max}")
            }
            Self::TooLarge {
                frame,
                len,
                capacity,
            } => write!(f, "{frame} length {len} exceeds arena capacity {capacity}"),
            Self::ConcurrentWriter => f.write_str("another writer is publishing to the PDO arena"),
            Self::Contended { retries } => {
                write!(f, "PDO arena snapshot not stable after {retries} retries")
            }
        }
    }
}

impl std::error::Error for ArenaError {}

/// 一帧 PDO 快照（读者拷贝出来的独立副本，不引用 arena 内部存储）。
///
/// `generation` 与 arena 的发布序号一致：越大越新；0 表示建好 arena 后还没发布过。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArenaSnapshot {
    /// 发布序号（每次成功 [`PdoArena::publish`] 加 1）。
    pub generation: u64,
    /// SM2（输出）方向的字节。
    pub sm2: Vec<u8>,
    /// SM3（输入）方向的字节。
    pub sm3: Vec<u8>,
}

impl ArenaSnapshot {
    /// 相对 `arena` 的当前发布序号是否已陈旧。
    pub fn is_stale(&self, arena: &PdoArena) -> bool {
        self.generation < arena.latest_generation()
    }
}

/// 双缓冲槽位：容量在构造时固定，之后只做原地原子拷贝。
#[derive(Debug)]
struct Slot {
    sm2: Box<[AtomicU8]>,
    sm3: Box<[AtomicU8]>,
    sm2_len: AtomicUsize,
    sm3_len: AtomicUsize,
}

impl Slot {
    fn new(sm2_capacity: usize, sm3_capacity: usize) -> Self {
        Self {
            sm2: (0..sm2_capacity).map(|_| AtomicU8::new(0)).collect(),
            sm3: (0..sm3_capacity).map(|_| AtomicU8::new(0)).collect(),
            sm2_len: AtomicUsize::new(0),
            sm3_len: AtomicUsize::new(0),
        }
    }
}

/// SM2/SM3 双缓冲 arena：单写者（实时环）发布，多读者（插件 worker）取快照。
///
/// ```rust
/// use plugin_host::arena::PdoArena;
///
/// let arena = PdoArena::new(8, 8).unwrap();
/// assert_eq!(arena.latest_generation(), 0);
/// let generation = arena.publish(&[0x11, 0x22], &[0x33]).unwrap();
/// assert_eq!(generation, 1);
///
/// let snapshot = arena.snapshot();
/// assert_eq!((snapshot.generation, snapshot.sm2.as_slice()), (1, &[0x11, 0x22][..]));
/// assert_eq!(snapshot.sm3, vec![0x33]);
/// assert!(!snapshot.is_stale(&arena));
/// ```
#[derive(Debug)]
pub struct PdoArena {
    slots: [Slot; 2],
    sm2_capacity: usize,
    sm3_capacity: usize,
    /// seqlock 序列号：偶数 = 无发布进行中，奇数 = 写者正在发布。
    sequence: AtomicU64,
    /// 已发布的快照数。
    generation: AtomicU64,
    /// 当前活动槽下标（0/1）。
    active: AtomicUsize,
    /// 单写者保护（CAS），避免并发写破坏序列号。
    publishing: AtomicBool,
}

impl PdoArena {
    /// 按两个方向各自的容量建一个空 arena（generation = 0，快照为空）。
    ///
    /// 容量在生命周期内固定；发布路径不分配内存。
    pub fn new(sm2_capacity: usize, sm3_capacity: usize) -> Result<Self, ArenaError> {
        for capacity in [sm2_capacity, sm3_capacity] {
            if capacity == 0 {
                return Err(ArenaError::ZeroCapacity);
            }
            if capacity > MAX_PDO_CAPACITY {
                return Err(ArenaError::CapacityTooLarge {
                    capacity,
                    max: MAX_PDO_CAPACITY,
                });
            }
        }
        Ok(Self {
            slots: [
                Slot::new(sm2_capacity, sm3_capacity),
                Slot::new(sm2_capacity, sm3_capacity),
            ],
            sm2_capacity,
            sm3_capacity,
            sequence: AtomicU64::new(0),
            generation: AtomicU64::new(0),
            active: AtomicUsize::new(0),
            publishing: AtomicBool::new(false),
        })
    }

    /// SM2（输出）方向的容量（字节）。
    pub fn sm2_capacity(&self) -> usize {
        self.sm2_capacity
    }

    /// SM3（输入）方向的容量（字节）。
    pub fn sm3_capacity(&self) -> usize {
        self.sm3_capacity
    }

    /// 当前发布序号（0 = 尚未发布）。
    pub fn latest_generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    /// 写者侧：发布一帧 SM2/SM3，返回新的 generation。
    ///
    /// 实时环调用：预检长度 + 单写者 CAS + 两次序列号自增 + 定长原子拷贝，
    /// **不分配、不等待读者**。任一步失败都不改变已发布快照。
    pub fn publish(&self, sm2: &[u8], sm3: &[u8]) -> Result<u64, ArenaError> {
        if sm2.len() > self.sm2_capacity {
            return Err(ArenaError::TooLarge {
                frame: Frame::Sm2,
                len: sm2.len(),
                capacity: self.sm2_capacity,
            });
        }
        if sm3.len() > self.sm3_capacity {
            return Err(ArenaError::TooLarge {
                frame: Frame::Sm3,
                len: sm3.len(),
                capacity: self.sm3_capacity,
            });
        }
        if self
            .publishing
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return Err(ArenaError::ConcurrentWriter);
        }

        // 进入临界区：序列号变奇数，读者会退避重试。
        self.sequence.fetch_add(1, Ordering::SeqCst);
        let active = self.active.load(Ordering::Acquire);
        let slot = &self.slots[active ^ 1];
        for (dst, src) in slot.sm2.iter().zip(sm2) {
            dst.store(*src, Ordering::Relaxed);
        }
        for (dst, src) in slot.sm3.iter().zip(sm3) {
            dst.store(*src, Ordering::Relaxed);
        }
        slot.sm2_len.store(sm2.len(), Ordering::Relaxed);
        slot.sm3_len.store(sm3.len(), Ordering::Relaxed);
        // 先发布槽位，再发布 generation；两者都用 Release，读者用 Acquire 读。
        self.active.store(active ^ 1, Ordering::Release);
        let generation = self.generation.fetch_add(1, Ordering::Release) + 1;
        // 退出临界区：序列号变偶数。
        self.sequence.fetch_add(1, Ordering::Release);
        self.publishing.store(false, Ordering::Release);
        Ok(generation)
    }

    /// 读者侧：取一帧一致快照；写者持续发布时可能自旋，用 [`Self::try_snapshot`] 有界重试。
    pub fn snapshot(&self) -> ArenaSnapshot {
        loop {
            if let Some(snapshot) = self.read_once() {
                return snapshot;
            }
            std::hint::spin_loop();
        }
    }

    /// 读者侧：最多重试 `retries` 次（0 表示只尝试一次），仍不稳定则返回
    /// [`ArenaError::Contended`]。
    pub fn try_snapshot(&self, retries: u32) -> Result<ArenaSnapshot, ArenaError> {
        for _ in 0..=retries {
            if let Some(snapshot) = self.read_once() {
                return Ok(snapshot);
            }
            std::hint::spin_loop();
        }
        Err(ArenaError::Contended { retries })
    }

    /// 单次 seqlock 读取：前后序列号一致（且为偶数）才返回快照。
    fn read_once(&self) -> Option<ArenaSnapshot> {
        let before = self.sequence.load(Ordering::Acquire);
        if before & 1 != 0 {
            return None;
        }
        let active = self.active.load(Ordering::Acquire);
        let slot = &self.slots[active];
        let generation = self.generation.load(Ordering::Acquire);
        let sm2_len = slot.sm2_len.load(Ordering::Relaxed).min(self.sm2_capacity);
        let sm3_len = slot.sm3_len.load(Ordering::Relaxed).min(self.sm3_capacity);
        let sm2 = (0..sm2_len)
            .map(|index| slot.sm2[index].load(Ordering::Relaxed))
            .collect();
        let sm3 = (0..sm3_len)
            .map(|index| slot.sm3[index].load(Ordering::Relaxed))
            .collect();
        // 读后复查：期间若有发布（序列号变化或变奇数），本帧作废重读。
        if self.sequence.load(Ordering::Acquire) != before {
            return None;
        }
        Some(ArenaSnapshot {
            generation,
            sm2,
            sm3,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread;

    use super::{ArenaError, Frame, PdoArena, MAX_PDO_CAPACITY};

    #[test]
    fn publish_and_snapshot_roundtrip() {
        let arena = PdoArena::new(8, 4).unwrap();
        assert_eq!(arena.latest_generation(), 0);
        assert_eq!(arena.sm2_capacity(), 8);
        assert_eq!(arena.sm3_capacity(), 4);

        // 未发布时快照为空。
        let empty = arena.snapshot();
        assert_eq!(
            (empty.generation, empty.sm2.len(), empty.sm3.len()),
            (0, 0, 0)
        );

        assert_eq!(arena.publish(&[1, 2, 3], &[9]).unwrap(), 1);
        let first = arena.snapshot();
        assert_eq!(first.generation, 1);
        assert_eq!(first.sm2, vec![1, 2, 3]);
        assert_eq!(first.sm3, vec![9]);
        assert!(!first.is_stale(&arena));

        // 第二次发布更短的数据：长度跟随新发布，不残留旧字节。
        assert_eq!(arena.publish(&[7], &[]).unwrap(), 2);
        let second = arena.snapshot();
        assert_eq!(second.generation, 2);
        assert_eq!(second.sm2, vec![7]);
        assert!(second.sm3.is_empty());
        assert!(first.is_stale(&arena));
        assert_eq!(arena.latest_generation(), 2);

        // 数据超过容量被拒绝，generation 不变。
        assert_eq!(
            arena.publish(&[0; 9], &[]),
            Err(ArenaError::TooLarge {
                frame: Frame::Sm2,
                len: 9,
                capacity: 8
            })
        );
        assert_eq!(
            arena.publish(&[], &[0; 5]),
            Err(ArenaError::TooLarge {
                frame: Frame::Sm3,
                len: 5,
                capacity: 4
            })
        );
        assert_eq!(arena.snapshot(), second);
    }

    #[test]
    fn invalid_capacity_is_rejected() {
        assert_eq!(PdoArena::new(0, 8).unwrap_err(), ArenaError::ZeroCapacity);
        assert_eq!(PdoArena::new(8, 0).unwrap_err(), ArenaError::ZeroCapacity);
        assert_eq!(
            PdoArena::new(MAX_PDO_CAPACITY + 1, 8).unwrap_err(),
            ArenaError::CapacityTooLarge {
                capacity: MAX_PDO_CAPACITY + 1,
                max: MAX_PDO_CAPACITY
            }
        );
        assert!(PdoArena::new(1, 1).is_ok());
    }

    #[test]
    fn concurrent_writer_is_rejected_without_corrupting_state() {
        let arena = PdoArena::new(4, 4).unwrap();
        // 模拟"另一个写者正在发布"：占用单写者标志。
        arena.publishing.store(true, Ordering::Relaxed);
        assert_eq!(arena.publish(&[1], &[2]), Err(ArenaError::ConcurrentWriter));
        // 序列号/快照未被这次失败污染，释放后仍可正常发布。
        assert_eq!(arena.latest_generation(), 0);
        arena.publishing.store(false, Ordering::Relaxed);
        assert_eq!(arena.publish(&[1], &[2]).unwrap(), 1);
    }

    /// 压测：写者连续发布，读者并发取快照——任何一帧都必须**整体**来自同一次发布。
    #[test]
    fn snapshot_is_never_torn_under_concurrent_publish() {
        const SM2_CAPACITY: usize = 64;
        const SM3_CAPACITY: usize = 17;
        const PUBLISHES: u64 = 20_000;

        fn pattern(generation: u64, index: usize) -> u8 {
            generation.wrapping_mul(131).wrapping_add(index as u64 * 7) as u8
        }
        fn sm2_len(generation: u64) -> usize {
            (generation as usize % SM2_CAPACITY) + 1
        }
        fn sm3_len(generation: u64) -> usize {
            ((generation / 3) as usize % SM3_CAPACITY) + 1
        }

        let arena = Arc::new(PdoArena::new(SM2_CAPACITY, SM3_CAPACITY).unwrap());
        let done = Arc::new(AtomicBool::new(false));

        let writer_arena = Arc::clone(&arena);
        let writer_done = Arc::clone(&done);
        let writer = thread::spawn(move || {
            for generation in 1..=PUBLISHES {
                let len2 = sm2_len(generation);
                let len3 = sm3_len(generation);
                let sm2: Vec<u8> = (0..len2).map(|i| pattern(generation, i)).collect();
                let sm3: Vec<u8> = (0..len3).map(|i| pattern(generation, i)).collect();
                writer_arena.publish(&sm2, &sm3).expect("发布失败");
            }
            writer_done.store(true, Ordering::Release);
        });

        let mut last_generation = 0u64;
        let mut reads = 0u64;
        while !done.load(Ordering::Acquire) {
            let snapshot = arena
                .try_snapshot(1_000_000)
                .expect("快照在 100 万次重试内不稳定");
            assert!(
                snapshot.generation >= last_generation,
                "generation 回退：{} -> {}",
                last_generation,
                snapshot.generation
            );
            last_generation = snapshot.generation;
            if snapshot.generation == 0 {
                assert!(snapshot.sm2.is_empty() && snapshot.sm3.is_empty());
                continue;
            }
            // 若发生撕裂（SM2 来自 gen A、SM3 来自 gen B），长度/字节断言必然失败。
            assert_eq!(snapshot.sm2.len(), sm2_len(snapshot.generation));
            assert_eq!(snapshot.sm3.len(), sm3_len(snapshot.generation));
            for (index, byte) in snapshot.sm2.iter().enumerate() {
                assert_eq!(
                    *byte,
                    pattern(snapshot.generation, index),
                    "SM2 撕裂：gen={} index={}",
                    snapshot.generation,
                    index
                );
            }
            for (index, byte) in snapshot.sm3.iter().enumerate() {
                assert_eq!(
                    *byte,
                    pattern(snapshot.generation, index),
                    "SM3 撕裂：gen={} index={}",
                    snapshot.generation,
                    index
                );
            }
            reads += 1;
        }
        writer.join().expect("写者线程 panic");

        let final_snapshot = arena.snapshot();
        assert_eq!(final_snapshot.generation, PUBLISHES);
        assert_eq!(final_snapshot.sm2.len(), sm2_len(PUBLISHES));
        assert!(reads > 0, "读者没有与写者重叠，压测无效");
    }
}
