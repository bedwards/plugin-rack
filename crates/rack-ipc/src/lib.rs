//! Inter-instance linking for plugin-rack.
//!
//! This crate implements the **registry layer** of the cross-instance IPC
//! story described in `research/ipc.md` §12 "Recommended architecture".
//!
//! # Scope of this module (issue #12, foundation PR)
//!
//! * A fixed-size shared-memory **slot table** that allows rack instances
//!   running inside the same machine (any host) to discover each other by
//!   a user-persisted `link_tag`.
//! * Each slot carries `pid`, `instance_uuid`, `link_tag`, and a monotonic
//!   `last_heartbeat_nanos`. An `alive` atomic disambiguates occupied vs
//!   free slots and lets the owner release the slot on drop.
//! * An allocation-free `heartbeat` call suitable for a periodic
//!   low-priority timer thread (NOT the audio thread — see §Non-goals).
//! * A slow-path `siblings` accessor that returns snapshots of other slots
//!   whose tag matches and whose heartbeat is within a TTL.
//!
//! # Non-goals (out of scope for this crate)
//!
//! * **No SPSC audio ring.** The cross-instance audio/state ring
//!   (research/ipc.md §11) is a separate concern and will live in its own
//!   module once issue #13 lands.
//! * **No audio-rate sharing.** Audio-rate cross-instance data is
//!   impossible inside a single block (SPEC.md §Two-track answer). Any
//!   future ring will be a *one-block-latency* publish channel used for
//!   analysis/metering only, and will never be invoked from the DAW
//!   audio callback of a sibling instance.
//! * **No local sockets, no OSC.** GUI-rate control lives on top of the
//!   ring, not here. See research/ipc.md §5.
//!
//! # Platform naming
//!
//! The registry segment name is constrained by:
//!
//! * **macOS App Sandbox:** POSIX shm names must be ≤ 31 bytes *including*
//!   leading slash (`PSHMNAMLEN` = 31). We use `/plugin-rack.reg.v1`
//!   (19 bytes) — well under the limit, version-tagged so a future schema
//!   break can bump `v2` and coexist.
//! * **Linux:** same POSIX shm, no length limit to speak of.
//! * **Windows:** unimplemented stub in this PR (API surface compiles so
//!   the workspace builds on Windows CI). Future patch will use
//!   `CreateFileMappingW` with a `Local\plugin-rack.reg.v1` name.
//!
//! # Timestamp source
//!
//! We use `CLOCK_MONOTONIC` via `libc::clock_gettime` on Unix. Rationale:
//!
//! * Monotonic — never jumps on wall-clock change / NTP slew.
//! * Alloc-free — pure syscall, no `Instant::now()` wrapper overhead and
//!   no hidden `thread_local!` init.
//! * Comparable across processes on the same host (it's a kernel counter).
//!
//! On Windows the future implementation will use
//! `QueryPerformanceCounter` + `QueryPerformanceFrequency` for an
//! equivalent monotonic source.

#![forbid(unsafe_op_in_unsafe_fn)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

#[cfg(unix)]
use std::ffi::CString;

pub const SLOT_COUNT: usize = 64;
pub const LINK_TAG_MAX: usize = 32;
pub const UUID_LEN: usize = 16;

/// Slot magic — used to detect a badly initialised segment.
const REGISTRY_MAGIC: [u8; 8] = *b"PLRACKR1";
const REGISTRY_VERSION: u32 = 1;

/// Shared-memory segment name (POSIX). 19 bytes, well under macOS
/// App-Sandbox `PSHMNAMLEN` (31).
#[cfg(unix)]
const REGISTRY_SHM_NAME: &str = "/plugin-rack.reg.v1";

/// User-visible label that identifies a group of linked rack instances.
///
/// Instances on the same `LinkTag` in the same DAW session will discover
/// each other and render a shared console view. Empty tag = not linked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LinkTag(pub String);

impl LinkTag {
    pub fn is_unlinked(&self) -> bool {
        self.0.is_empty()
    }
}

/// One registry slot. `repr(C)` with fixed layout so layouts agree across
/// independently-built binaries that happen to share a segment.
///
/// `alive`: atomic `u32`. `0` = free, `1` = occupied. We access it through
/// `AtomicU32::from_ptr` (Rust 1.75+) so the raw struct stays POD-like.
///
/// `_pad`: reserved to take the struct size to a pleasant 96 bytes. Do not
/// remove without bumping `REGISTRY_VERSION` and the magic.
#[repr(C)]
#[derive(Clone, Copy)]
struct Slot {
    alive: u32,
    pid: u32,
    last_heartbeat_nanos: u64,
    instance_uuid: [u8; UUID_LEN],
    link_tag: [u8; LINK_TAG_MAX],
    _pad: [u8; 32],
}

// SAFETY: POD — no references, no niches, every bit pattern is valid.
// We use bytemuck to enforce this at compile time via Zeroable.
unsafe impl bytemuck::Zeroable for Slot {}

const _: () = assert!(std::mem::size_of::<Slot>() == 96);

/// Header block preceding the slot table.
#[repr(C)]
#[derive(Clone, Copy)]
struct Header {
    magic: [u8; 8],
    version: u32,
    slot_count: u32,
    _pad: [u8; 48],
}

unsafe impl bytemuck::Zeroable for Header {}

const _: () = assert!(std::mem::size_of::<Header>() == 64);

/// Total segment size: header + `SLOT_COUNT` slots.
const SEGMENT_SIZE: usize =
    std::mem::size_of::<Header>() + SLOT_COUNT * std::mem::size_of::<Slot>();

/// Plain-data snapshot of a slot. No raw pointers, safe to hand to the GUI.
#[derive(Debug, Clone)]
pub struct SlotSnapshot {
    pub pid: u32,
    pub instance_uuid: [u8; UUID_LEN],
    /// Link tag as a UTF-8 string with trailing NULs trimmed.
    pub link_tag: String,
    pub last_heartbeat_nanos: u64,
}

// ─────────────────────────────────────────────────────────────────────────────
// Platform: Unix (macOS + Linux) uses `shm_open` + `mmap`.
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(unix)]
mod platform {
    use super::{REGISTRY_SHM_NAME, SEGMENT_SIZE};
    use anyhow::{Context, Result, bail};
    use memmap2::{MmapMut, MmapOptions};
    use std::ffi::CString;
    use std::os::fd::{FromRawFd, OwnedFd};

    /// Open-or-create the registry shm segment. Idempotent: multiple callers
    /// may race; the loser just opens what the winner created.
    pub(super) fn open_or_create() -> Result<(MmapMut, bool)> {
        // SAFETY: `shm_open` is a POSIX syscall; we pass a NUL-terminated
        // name and standard O_CREAT|O_RDWR flags.
        let cname =
            CString::new(REGISTRY_SHM_NAME).context("registry shm name contained a NUL byte")?;

        let mode = 0o600_u32;
        let flags = libc::O_RDWR | libc::O_CREAT;

        // `shm_open` returns an fd >= 0 on success, -1 on error.
        let fd = unsafe { libc::shm_open(cname.as_ptr(), flags, mode as libc::c_uint) };
        if fd < 0 {
            let err = std::io::Error::last_os_error();
            bail!("shm_open({}) failed: {}", REGISTRY_SHM_NAME, err);
        }
        // SAFETY: `fd` was just returned by `shm_open` and we're the sole
        // owner — wrap in `OwnedFd` so it gets closed on drop / panic.
        let owned = unsafe { OwnedFd::from_raw_fd(fd) };

        // `ftruncate` is idempotent if the file already has this size, which
        // is the common case for the second-attacher. On a fresh segment it
        // zero-fills, giving us clean all-zero slots.
        let trunc = unsafe { libc::ftruncate(fd, SEGMENT_SIZE as libc::off_t) };
        if trunc < 0 {
            let err = std::io::Error::last_os_error();
            bail!(
                "ftruncate({}) to {} bytes failed: {}",
                REGISTRY_SHM_NAME,
                SEGMENT_SIZE,
                err
            );
        }

        // mmap.
        let mmap = unsafe { MmapOptions::new().len(SEGMENT_SIZE).map_mut(&owned) }
            .with_context(|| format!("mmap of {} bytes failed", SEGMENT_SIZE))?;

        // `owned` is dropped here → fd is closed. mmap keeps the mapping.
        Ok((mmap, true))
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::SEGMENT_SIZE;
    use anyhow::{Result, bail};
    use memmap2::MmapMut;

    /// Windows stub. Tracked by issue #12 follow-up. Must at least compile so
    /// the workspace builds on CI; calling it panics with a clear message.
    pub(super) fn open_or_create() -> Result<(MmapMut, bool)> {
        // Suppress the unused-import warning — `SEGMENT_SIZE` is retained so
        // a future `CreateFileMappingW` impl can use it without churn.
        let _ = SEGMENT_SIZE;
        bail!("rack-ipc SharedRegistry: Windows implementation pending (see issue #12)")
    }
}

#[cfg(not(any(unix, target_os = "windows")))]
mod platform {
    use anyhow::{Result, bail};
    use memmap2::MmapMut;

    pub(super) fn open_or_create() -> Result<(MmapMut, bool)> {
        bail!("rack-ipc SharedRegistry: unsupported platform")
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SharedRegistry
// ─────────────────────────────────────────────────────────────────────────────

/// Shared-memory slot table.
///
/// Attaches to (or creates) a single process-wide shm segment. All methods
/// are safe to call from any thread, but `heartbeat` is the only one
/// intended for a hot-ish path and is documented alloc-free.
///
/// # Audio thread safety
///
/// **Do NOT call `claim_slot`, `siblings`, or `open_or_create` from the
/// audio thread.** They may allocate or syscall. `heartbeat` is the only
/// method that is safe from a moderately hot timer thread — even so, the
/// design intent is a ~500 ms cadence from a low-priority discovery thread,
/// never from the DAW process callback.
pub struct SharedRegistry {
    mmap: Arc<memmap2::MmapMut>,
}

impl SharedRegistry {
    /// Attach to the process-wide registry segment, creating it if absent.
    ///
    /// Not alloc-free; call from plugin `initialize()`, not the audio path.
    pub fn open_or_create() -> anyhow::Result<Self> {
        let (mmap, created) = platform::open_or_create()?;
        let mmap = Arc::new(mmap);

        // If we were the creator (or first-time initialiser of a stale
        // segment), stamp the header. This is a single store; if two
        // processes race, both write the same bytes.
        //
        // We detect "uninitialised or stale" by checking the magic field.
        // A zero-filled segment (fresh shm) has all-zero magic, which
        // trivially fails the equality check.
        Self::init_header_if_needed(mmap.as_ref());
        // `created` is informational for tests; keep the var named so the
        // compiler does not warn on non-test builds.
        let _ = created;

        Ok(Self { mmap })
    }

    /// Write the header block if the magic is not yet our known value.
    fn init_header_if_needed(mmap: &memmap2::MmapMut) {
        // SAFETY: the segment is at least `size_of::<Header>` bytes long
        // (we ftruncate to SEGMENT_SIZE before mapping). Accessing the
        // header as a raw pointer is sound. We use volatile writes so that
        // another process reading concurrently sees the fully-written
        // header in memory order.
        let ptr = mmap.as_ptr() as *mut Header;
        // Read current header.
        let current = unsafe { ptr.read_volatile() };
        if current.magic == REGISTRY_MAGIC && current.version == REGISTRY_VERSION {
            return;
        }
        let fresh = Header {
            magic: REGISTRY_MAGIC,
            version: REGISTRY_VERSION,
            slot_count: SLOT_COUNT as u32,
            _pad: [0; 48],
        };
        unsafe { ptr.write_volatile(fresh) };
    }

    /// Pointer to the first slot. All slot access goes through this base.
    ///
    /// The returned pointer is only dereferenced through atomic ops or
    /// `read_volatile` / `write_volatile` to avoid data races.
    fn slots_ptr(&self) -> *mut Slot {
        let base = self.mmap.as_ptr() as *mut u8;
        // SAFETY: the mapping is SEGMENT_SIZE bytes long; the slots start
        // at offset size_of::<Header>() and there are SLOT_COUNT of them.
        unsafe { base.add(std::mem::size_of::<Header>()) as *mut Slot }
    }

    /// Claim a free slot, stamping it with `pid`, a fresh `instance_uuid`,
    /// the given `link_tag`, and the current monotonic timestamp.
    ///
    /// Returns an RAII handle that zeroes `alive` on drop, releasing the
    /// slot for reuse.
    ///
    /// Not alloc-free — call once at plugin `initialize()`.
    pub fn claim_slot(&self, link_tag: &[u8]) -> anyhow::Result<SlotHandle> {
        if link_tag.len() > LINK_TAG_MAX {
            anyhow::bail!(
                "link_tag too long ({} bytes, max {})",
                link_tag.len(),
                LINK_TAG_MAX
            );
        }

        let slots = self.slots_ptr();
        for idx in 0..SLOT_COUNT {
            // SAFETY: `slots` points to a SLOT_COUNT-long array; idx is
            // bounded; the field offset of `alive` is the first member.
            let alive_ptr = unsafe { core::ptr::addr_of_mut!((*slots.add(idx)).alive) };
            let alive_atomic = unsafe { AtomicU32::from_ptr(alive_ptr) };
            match alive_atomic.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Relaxed) {
                Ok(_) => {
                    // We own slot `idx`. Fill in the rest of the fields.
                    // These writes happen AFTER the successful CAS, so a
                    // scanner that has read `alive == 1` but not yet
                    // observed our tag/uuid will simply see zeros or the
                    // prior contents — acceptable because the scanner also
                    // checks `last_heartbeat_nanos` against the TTL.
                    //
                    // We write tag/uuid/pid BEFORE the first heartbeat so
                    // that any scanner observing a non-zero heartbeat
                    // observes consistent identity fields.
                    let mut tag_buf = [0u8; LINK_TAG_MAX];
                    tag_buf[..link_tag.len()].copy_from_slice(link_tag);
                    let uuid = fresh_uuid();

                    // Write tag.
                    let tag_ptr = unsafe { core::ptr::addr_of_mut!((*slots.add(idx)).link_tag) };
                    unsafe { tag_ptr.write_volatile(tag_buf) };

                    // Write uuid.
                    let uuid_ptr =
                        unsafe { core::ptr::addr_of_mut!((*slots.add(idx)).instance_uuid) };
                    unsafe { uuid_ptr.write_volatile(uuid) };

                    // Write pid.
                    let pid_ptr = unsafe { core::ptr::addr_of_mut!((*slots.add(idx)).pid) };
                    unsafe { pid_ptr.write_volatile(std::process::id()) };

                    // Now the first heartbeat. Use Release so other
                    // threads observing a non-zero heartbeat also see the
                    // identity fields above.
                    let hb_ptr =
                        unsafe { core::ptr::addr_of_mut!((*slots.add(idx)).last_heartbeat_nanos) };
                    let now = monotonic_nanos();
                    // Release fence ensures the tag/uuid/pid writes above
                    // are visible before the heartbeat value.
                    std::sync::atomic::fence(Ordering::Release);
                    unsafe { hb_ptr.write_volatile(now) };

                    return Ok(SlotHandle {
                        mmap: Arc::clone(&self.mmap),
                        slot_idx: idx,
                        instance_uuid: uuid,
                    });
                }
                Err(_) => continue,
            }
        }
        anyhow::bail!("registry full: no free slot among {} slots", SLOT_COUNT)
    }

    /// Update the heartbeat timestamp on the given slot.
    ///
    /// # Allocation-free contract
    ///
    /// This function performs exactly:
    ///
    /// 1. One `clock_gettime(CLOCK_MONOTONIC)` syscall (Unix) — no Rust
    ///    allocation, kernel-side only.
    /// 2. One `write_volatile` to the slot's `last_heartbeat_nanos`.
    /// 3. One `load(Relaxed)` on the slot's `alive` atomic (defensive —
    ///    we skip the write if the slot has been reclaimed out from
    ///    under us).
    ///
    /// There is no `Vec`, `Box`, `String`, `format!`, `println!`, nor
    /// file / socket IO on this path. It is safe to call from the audio
    /// thread *in principle*, though the intended use is a ~500 ms
    /// heartbeat timer per research/ipc.md §12.
    #[inline]
    pub fn heartbeat(&self, handle: &SlotHandle) {
        let slots = self.slots_ptr();
        debug_assert!(handle.slot_idx < SLOT_COUNT);
        // Defensive: if the slot was somehow reclaimed (shouldn't happen
        // while we hold the handle), don't stomp it.
        let alive_ptr = unsafe { core::ptr::addr_of_mut!((*slots.add(handle.slot_idx)).alive) };
        let alive_atomic = unsafe { AtomicU32::from_ptr(alive_ptr) };
        if alive_atomic.load(Ordering::Relaxed) == 0 {
            return;
        }
        let now = monotonic_nanos();
        let hb_ptr =
            unsafe { core::ptr::addr_of_mut!((*slots.add(handle.slot_idx)).last_heartbeat_nanos) };
        unsafe { hb_ptr.write_volatile(now) };
    }

    /// Return snapshots of every live slot whose `link_tag` matches AND
    /// whose heartbeat is within `ttl_nanos` of `now_nanos`. Excludes the
    /// caller's own slot if `exclude_uuid` is provided.
    ///
    /// **Not alloc-free.** This allocates a `Vec`. It is called from the
    /// GUI / slow path at ≤ 30 Hz per research/ipc.md §5.
    pub fn siblings(&self, link_tag: &[u8], now_nanos: u64, ttl_nanos: u64) -> Vec<SlotSnapshot> {
        self.siblings_excluding(link_tag, now_nanos, ttl_nanos, None)
    }

    /// Same as `siblings`, but skip any slot whose `instance_uuid` matches
    /// `exclude_uuid`. Convenience for a caller who doesn't want to see
    /// their own entry.
    pub fn siblings_excluding(
        &self,
        link_tag: &[u8],
        now_nanos: u64,
        ttl_nanos: u64,
        exclude_uuid: Option<[u8; UUID_LEN]>,
    ) -> Vec<SlotSnapshot> {
        let slots = self.slots_ptr();
        let mut out = Vec::new();
        for idx in 0..SLOT_COUNT {
            let alive_ptr = unsafe { core::ptr::addr_of_mut!((*slots.add(idx)).alive) };
            let alive_atomic = unsafe { AtomicU32::from_ptr(alive_ptr) };
            if alive_atomic.load(Ordering::Acquire) == 0 {
                continue;
            }
            // Read the rest of the slot non-atomically. We do it via
            // `read_volatile` on each field pointer to avoid tearing
            // guarantees across non-atomic reads; the slot owner doesn't
            // change tag/uuid/pid after claim, so this is torn-free in
            // practice.
            let slot = unsafe { slots.add(idx).read_volatile() };

            // Heartbeat TTL check.
            let hb = slot.last_heartbeat_nanos;
            if now_nanos.saturating_sub(hb) > ttl_nanos {
                continue;
            }
            // Tag match.
            if !tag_eq(&slot.link_tag, link_tag) {
                continue;
            }
            // Self-exclusion.
            if let Some(excl) = exclude_uuid
                && slot.instance_uuid == excl
            {
                continue;
            }

            out.push(SlotSnapshot {
                pid: slot.pid,
                instance_uuid: slot.instance_uuid,
                link_tag: trimmed_tag(&slot.link_tag),
                last_heartbeat_nanos: slot.last_heartbeat_nanos,
            });
        }
        out
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SlotHandle — RAII ownership token for a claimed slot.
// ─────────────────────────────────────────────────────────────────────────────

/// RAII handle for a claimed registry slot. Dropping it zeroes `alive`,
/// freeing the slot for reuse.
pub struct SlotHandle {
    mmap: Arc<memmap2::MmapMut>,
    slot_idx: usize,
    instance_uuid: [u8; UUID_LEN],
}

impl SlotHandle {
    /// The randomly-generated UUID written into this slot at claim time.
    ///
    /// Useful for `siblings_excluding` so a caller can filter themselves
    /// out of the scan result.
    pub fn instance_uuid(&self) -> [u8; UUID_LEN] {
        self.instance_uuid
    }

    /// 0-based slot index within the registry table. Test-only.
    #[cfg(test)]
    fn slot_idx(&self) -> usize {
        self.slot_idx
    }
}

impl Drop for SlotHandle {
    fn drop(&mut self) {
        // Zero `alive`. This is the entire release step — siblings() will
        // observe the CAS-released state and drop us from the scan. We
        // don't bother clearing the other fields; the next claimant will
        // overwrite them, and a stale reader looking at an already-freed
        // slot will filter it out via the `alive == 0` check.
        let base = self.mmap.as_ptr() as *mut u8;
        let slot_ptr = unsafe { base.add(std::mem::size_of::<Header>()) as *mut Slot };
        let alive_ptr = unsafe { core::ptr::addr_of_mut!((*slot_ptr.add(self.slot_idx)).alive) };
        let alive_atomic = unsafe { AtomicU32::from_ptr(alive_ptr) };
        alive_atomic.store(0, Ordering::Release);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Utility
// ─────────────────────────────────────────────────────────────────────────────

/// Monotonic nanosecond timestamp.
///
/// Unix: `clock_gettime(CLOCK_MONOTONIC)`. Allocation-free — the kernel
/// writes into a stack `timespec`.
///
/// Windows (future): `QueryPerformanceCounter` scaled by
/// `QueryPerformanceFrequency`.
#[inline]
pub fn monotonic_nanos() -> u64 {
    #[cfg(unix)]
    {
        let mut ts: libc::timespec = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY: `ts` is valid for writing; `CLOCK_MONOTONIC` is defined
        // on every Unix we support.
        let rc = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
        if rc != 0 {
            // Defensive fallback — this should never fail on a supported OS.
            return 0;
        }
        (ts.tv_sec as u64)
            .saturating_mul(1_000_000_000)
            .saturating_add(ts.tv_nsec as u64)
    }
    #[cfg(not(unix))]
    {
        // Windows stub for future implementation. Returns zero — the
        // registry's Windows code path is also unimplemented.
        0
    }
}

/// Generate a fresh hex-encoded link tag suitable for storing in plugin
/// state. 24 hex chars (96 bits of entropy), well under `LINK_TAG_MAX`.
///
/// Intended use: a newly-instantiated plugin-rack instance gets a unique
/// tag so it does NOT auto-link with an unrelated sibling. The user edits
/// this later via the GUI to opt in to a group.
pub fn fresh_link_tag() -> String {
    let bytes = fresh_uuid();
    let mut s = String::with_capacity(24);
    for b in &bytes[..12] {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Fresh pseudo-random UUID-ish 16 bytes. We don't depend on the `uuid`
/// crate here because the value is opaque — we just need 128 bits of
/// per-instance identity that doesn't collide in a single DAW session.
///
/// Uses a small PRNG seeded from the OS + PID + time.
fn fresh_uuid() -> [u8; UUID_LEN] {
    // Mix PID, monotonic time, and address-of-local to get per-call entropy.
    let pid = std::process::id() as u64;
    let now = monotonic_nanos();
    let local: u64 = &pid as *const _ as usize as u64;
    let mut s = pid ^ now.rotate_left(13) ^ local.rotate_left(29);
    let mut out = [0u8; UUID_LEN];
    for chunk in out.chunks_exact_mut(8) {
        // SplitMix64 — compact, good enough for identity, no deps.
        s = s.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = s;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^= z >> 31;
        chunk.copy_from_slice(&z.to_le_bytes());
    }
    out
}

/// Compare a fixed-size tag buffer to a query. The buffer is padded with
/// trailing NULs; shorter queries still match if they're followed by NULs.
fn tag_eq(buf: &[u8; LINK_TAG_MAX], query: &[u8]) -> bool {
    if query.len() > LINK_TAG_MAX {
        return false;
    }
    if &buf[..query.len()] != query {
        return false;
    }
    // Remaining bytes must be NUL — else the buffer has a longer tag.
    buf[query.len()..].iter().all(|&b| b == 0)
}

/// Convert a fixed-size NUL-padded tag buffer to a `String`. Any non-UTF-8
/// bytes are replaced with U+FFFD; the caller writes a pure-ASCII tag in
/// practice so this only ever kicks in on a corrupt segment.
fn trimmed_tag(buf: &[u8; LINK_TAG_MAX]) -> String {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(LINK_TAG_MAX);
    String::from_utf8_lossy(&buf[..end]).into_owned()
}

// ─────────────────────────────────────────────────────────────────────────────
// Unix unlink helper (for tests + graceful shutdown)
// ─────────────────────────────────────────────────────────────────────────────

/// Remove the shm segment from the system. No-op on Windows (stub).
///
/// Used by tests to ensure a clean slate. In production we don't usually
/// unlink on exit because another instance may still hold the segment —
/// the kernel reclaims it when the last mapping drops. Callers that want
/// a deterministic wipe (e.g., integration tests) can call this, but
/// should serialize across tests to avoid stepping on each other.
#[cfg(unix)]
pub fn unlink_registry() -> std::io::Result<()> {
    let cname = CString::new(REGISTRY_SHM_NAME)
        .map_err(|e| std::io::Error::other(format!("invalid shm name: {e}")))?;
    // SAFETY: `shm_unlink` returns 0 on success, -1 on error. It is safe
    // to call even when the segment does not exist (we ignore ENOENT).
    let rc = unsafe { libc::shm_unlink(cname.as_ptr()) };
    if rc != 0 {
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() != Some(libc::ENOENT) {
            return Err(err);
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn unlink_registry() -> std::io::Result<()> {
    // Windows file-mapping objects are refcounted; there's no explicit
    // unlink. Matching signature keeps cross-platform test code clean.
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Tests share a single OS-wide segment, so we serialize them with a
    // mutex to avoid cross-test interference. Each test unlinks + opens
    // fresh to start from a known state.
    //
    // This is slightly crude but avoids pulling in a test-only dep like
    // `serial_test`. The set of registry tests is small.
    use std::sync::Mutex;
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn fresh_registry() -> SharedRegistry {
        #[cfg(unix)]
        {
            let _ = unlink_registry();
        }
        SharedRegistry::open_or_create().expect("open registry")
    }

    #[cfg(unix)]
    #[test]
    fn claim_two_slots_same_tag_discovers_both() {
        let _g = TEST_LOCK.lock().unwrap();
        let reg = fresh_registry();
        let a = reg.claim_slot(b"test-link-1").expect("claim a");
        let b = reg.claim_slot(b"test-link-1").expect("claim b");

        assert_ne!(a.slot_idx(), b.slot_idx(), "slots must be distinct");
        assert_ne!(a.instance_uuid(), b.instance_uuid(), "uuids must differ");

        // From slot A's perspective, B should show up as a sibling.
        let now = monotonic_nanos();
        let ttl = 10_000_000_000; // 10 s — plenty of headroom.
        let sibs = reg.siblings_excluding(b"test-link-1", now, ttl, Some(a.instance_uuid()));
        assert_eq!(sibs.len(), 1, "should see exactly slot B");
        assert_eq!(sibs[0].instance_uuid, b.instance_uuid());
        assert_eq!(sibs[0].link_tag, "test-link-1");
        assert_eq!(sibs[0].pid, std::process::id());
    }

    #[cfg(unix)]
    #[test]
    fn dropped_slot_is_removed_from_siblings() {
        let _g = TEST_LOCK.lock().unwrap();
        let reg = fresh_registry();
        let a = reg.claim_slot(b"test-link-2").expect("claim a");
        let my_uuid = a.instance_uuid();
        {
            let _b = reg.claim_slot(b"test-link-2").expect("claim b");
            let now = monotonic_nanos();
            let sibs = reg.siblings_excluding(b"test-link-2", now, 10_000_000_000, Some(my_uuid));
            assert_eq!(sibs.len(), 1, "saw B while held");
        } // b drops here → alive=0.

        let now = monotonic_nanos();
        let sibs = reg.siblings_excluding(b"test-link-2", now, 10_000_000_000, Some(my_uuid));
        assert_eq!(sibs.len(), 0, "B should be gone after drop");
        drop(a);
    }

    #[cfg(unix)]
    #[test]
    fn stale_heartbeat_hides_slot_via_ttl() {
        let _g = TEST_LOCK.lock().unwrap();
        let reg = fresh_registry();
        let a = reg.claim_slot(b"test-link-3").expect("claim a");
        let b = reg.claim_slot(b"test-link-3").expect("claim b");

        // Call siblings with a 1-nanosecond TTL and `now` far in the past
        // — both slots' heartbeats are "in the future" relative to `now`,
        // so `now.saturating_sub(hb) == 0 <= ttl` — they show up.
        let sibs = reg.siblings_excluding(b"test-link-3", 0, 1, Some(a.instance_uuid()));
        assert_eq!(sibs.len(), 1);

        // Now call with `now` well past both heartbeats and TTL = 0 — no
        // slot survives.
        let future = monotonic_nanos() + 60_000_000_000; // +60 s
        let sibs = reg.siblings_excluding(b"test-link-3", future, 0, Some(a.instance_uuid()));
        assert_eq!(sibs.len(), 0, "everyone is stale past TTL");
        drop(b);
    }

    #[cfg(unix)]
    #[test]
    fn heartbeat_updates_timestamp_monotonically() {
        let _g = TEST_LOCK.lock().unwrap();
        let reg = fresh_registry();
        let a = reg.claim_slot(b"test-link-hb").expect("claim a");

        let slots = reg.slots_ptr();
        let hb0 = unsafe {
            core::ptr::addr_of!((*slots.add(a.slot_idx())).last_heartbeat_nanos).read_volatile()
        };
        // Spin a tiny bit to ensure the monotonic clock has ticked.
        std::thread::sleep(std::time::Duration::from_millis(2));
        reg.heartbeat(&a);
        let hb1 = unsafe {
            core::ptr::addr_of!((*slots.add(a.slot_idx())).last_heartbeat_nanos).read_volatile()
        };
        assert!(hb1 > hb0, "heartbeat must advance the timestamp");
    }

    #[cfg(unix)]
    #[test]
    fn siblings_ignores_other_tag() {
        let _g = TEST_LOCK.lock().unwrap();
        let reg = fresh_registry();
        let a = reg.claim_slot(b"group-a").expect("claim a");
        let _b = reg.claim_slot(b"group-b").expect("claim b");

        let sibs = reg.siblings_excluding(
            b"group-a",
            monotonic_nanos(),
            10_000_000_000,
            Some(a.instance_uuid()),
        );
        assert_eq!(sibs.len(), 0, "group-b must not appear in group-a scan");
    }

    #[test]
    fn link_tag_unlinked_default() {
        assert!(LinkTag::default().is_unlinked());
        assert!(!LinkTag("x".into()).is_unlinked());
    }

    #[test]
    fn tag_equality() {
        let mut buf = [0u8; LINK_TAG_MAX];
        buf[..3].copy_from_slice(b"abc");
        assert!(tag_eq(&buf, b"abc"));
        assert!(!tag_eq(&buf, b"abcd"));
        assert!(!tag_eq(&buf, b"ab"));
    }

    #[test]
    fn trimmed_tag_roundtrip() {
        let mut buf = [0u8; LINK_TAG_MAX];
        buf[..5].copy_from_slice(b"hello");
        assert_eq!(trimmed_tag(&buf), "hello");
    }

    #[test]
    fn header_size_stable() {
        // If these change, bump REGISTRY_VERSION and the magic so stale
        // mappings get reinitialised rather than misinterpreted.
        assert_eq!(std::mem::size_of::<Header>(), 64);
        assert_eq!(std::mem::size_of::<Slot>(), 96);
    }

    #[cfg(unix)]
    #[test]
    fn claim_too_long_tag_errors() {
        let _g = TEST_LOCK.lock().unwrap();
        let reg = fresh_registry();
        let too_long = [b'x'; LINK_TAG_MAX + 1];
        assert!(reg.claim_slot(&too_long).is_err());
    }
}
