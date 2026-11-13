//! Memory owned by an admitted DELEGATED setup. No socket or account policy lives here.
use std::io;
use std::ptr::NonNull;
use std::sync::OnceLock;
use zeroize::{Zeroize, Zeroizing};

/// Created only after the caller checks hosted signing and origin support.
/// Preparation precedes receiving or opening any DELEGATED key material.
#[derive(Debug)]
pub struct DelegatedMemory(());

impl DelegatedMemory {
    pub fn prepare(_venue: &str, hosted_supported: bool) -> io::Result<Self> {
        if !hosted_supported {
            return Err(io::ErrorKind::Unsupported.into());
        }
        static READY: OnceLock<Result<(), io::ErrorKind>> = OnceLock::new();
        READY
            .get_or_init(|| prepare_process().map_err(|error| error.kind()))
            .as_ref()
            .map_err(|kind| io::Error::from(*kind))?;
        Ok(Self(()))
    }

    /// Prefer opening directly into this buffer when the opener supports it.
    pub fn zeroed(&self, len: usize) -> io::Result<CandidateCopy> {
        SecretCopy::allocate(len)
    }

    /// Consume an existing buffer, wiping it on success and every error path.
    pub fn copy(&self, bytes: Vec<u8>) -> io::Result<CandidateCopy> {
        let bytes = Zeroizing::new(bytes);
        let mut copy = self.zeroed(bytes.len())?;
        copy.as_bytes_mut().copy_from_slice(&bytes);
        Ok(copy)
    }
}

/// A candidate and its resident copy have the same explicit memory ownership.
pub type CandidateCopy = SecretCopy;
pub type ResidentCopy = SecretCopy;

#[cfg(test)]
type ReleaseCheck = Box<dyn FnOnce(&[u8]) + Send + Sync>;

/// An independently allocated, redacted buffer. It has no serialization or implicit clone.
pub struct SecretCopy {
    address: NonNull<u8>,
    mapped_len: usize,
    len: usize,
    locked: bool,
    #[cfg(test)]
    before_release: Option<ReleaseCheck>,
}

// SAFETY: each allocation is exclusively owned, never resized or aliased mutably.
// Shared access exposes immutable bytes; mutation requires an exclusive borrow.
unsafe impl Send for SecretCopy {}
unsafe impl Sync for SecretCopy {}

impl SecretCopy {
    fn allocate(len: usize) -> io::Result<Self> {
        #[cfg(unix)]
        {
            // SAFETY: sysconf has no pointer arguments and does not change process state.
            let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
            let page = usize::try_from(page)
                .ok()
                .filter(|page| *page > 0)
                .ok_or_else(|| io::Error::from(io::ErrorKind::Unsupported))?;
            let mapped_len = len
                .max(1)
                .checked_next_multiple_of(page)
                .filter(|len| *len <= isize::MAX as usize)
                .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
            // SAFETY: fresh anonymous mapping, no fixed address or file descriptor.
            let raw = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    mapped_len,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                    -1,
                    0,
                )
            };
            if raw == libc::MAP_FAILED {
                return Err(io::Error::last_os_error());
            }
            let Some(address) = NonNull::new(raw.cast::<u8>()) else {
                // SAFETY: release the successful mapping before refusing a null base.
                unsafe {
                    libc::munmap(raw, mapped_len);
                }
                return Err(io::ErrorKind::OutOfMemory.into());
            };
            // SAFETY: the mapping is alive and independently page aligned.
            let locked = unsafe { libc::mlock(raw, mapped_len) } == 0;
            if !locked {
                static WARNED: std::sync::Once = std::sync::Once::new();
                WARNED.call_once(|| {
                    crate::state::Log::warn("DELEGATED memory locking unavailable; continuing")
                });
            }
            Ok(Self {
                address,
                mapped_len,
                len,
                locked,
                #[cfg(test)]
                before_release: None,
            })
        }
        #[cfg(not(unix))]
        {
            let _ = len;
            Err(io::ErrorKind::Unsupported.into())
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        // SAFETY: the mapping outlives this borrow; len is within its allocation.
        unsafe { std::slice::from_raw_parts(self.address.as_ptr(), self.len) }
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        // SAFETY: exclusive borrow of an exclusively owned, live mapping.
        unsafe { std::slice::from_raw_parts_mut(self.address.as_ptr(), self.len) }
    }

    pub fn is_locked(&self) -> bool {
        self.locked
    }

    /// Finish an in-place opening without retaining its unused suffix.
    pub fn truncate(&mut self, len: usize) {
        if let Some(tail) = self.as_bytes_mut().get_mut(len..) {
            tail.zeroize();
            self.len = len;
        }
    }

    /// Each additional copy owns separate pages and releases only those pages.
    pub fn try_clone(&self) -> io::Result<ResidentCopy> {
        let mut copy = Self::allocate(self.len)?;
        copy.as_bytes_mut().copy_from_slice(self.as_bytes());
        Ok(copy)
    }
}

impl std::fmt::Debug for SecretCopy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DELEGATED memory [REDACTED]")
    }
}

impl Zeroize for SecretCopy {
    fn zeroize(&mut self) {
        // SAFETY: exclusive access to the complete live allocation, including padding.
        unsafe { std::slice::from_raw_parts_mut(self.address.as_ptr(), self.mapped_len) }.zeroize();
        self.len = 0;
    }
}

impl Drop for SecretCopy {
    fn drop(&mut self) {
        self.zeroize();
        #[cfg(test)]
        if let Some(check) = self.before_release.take() {
            // SAFETY: the allocation is still live; the observer cannot retain the borrow.
            check(unsafe { std::slice::from_raw_parts(self.address.as_ptr(), self.mapped_len) });
        }
        #[cfg(unix)]
        // SAFETY: this owner alone releases its mapping, after wiping it.
        unsafe {
            if self.locked {
                libc::munlock(self.address.as_ptr().cast(), self.mapped_len);
            }
            libc::munmap(self.address.as_ptr().cast(), self.mapped_len);
        }
    }
}

fn prepare_process() -> io::Result<()> {
    #[cfg(unix)]
    {
        let limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: a valid initialized limit; this only lowers this process's limit.
        if unsafe { libc::setrlimit(libc::RLIMIT_CORE, &limit) } != 0 {
            return Err(io::Error::last_os_error());
        }
        #[cfg(target_os = "linux")]
        // SAFETY: PR_SET_DUMPABLE accepts a scalar flag and no pointer arguments.
        if unsafe {
            libc::prctl(
                libc::PR_SET_DUMPABLE,
                0 as libc::c_ulong,
                0 as libc::c_ulong,
                0 as libc::c_ulong,
                0 as libc::c_ulong,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        Err(io::ErrorKind::Unsupported.into())
    }
}

#[cfg(all(test, unix))]
mod opening_tests {
    use super::*;

    #[test]
    fn truncation_preserves_prefix_and_erases_suffix() {
        let mut copy = SecretCopy::allocate(9).unwrap();
        copy.as_bytes_mut().copy_from_slice(b"synthetic");
        copy.truncate(4);
        assert_eq!(copy.as_bytes(), b"synt");
        // Restore the original in-bounds view to observe the removed bytes.
        copy.len = 9;
        assert_eq!(copy.as_bytes(), b"synt\0\0\0\0\0");
        copy.truncate(usize::MAX);
        assert_eq!(copy.as_bytes().len(), 9);
        copy.truncate(0);
        assert!(copy.as_bytes().is_empty());
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::process::Command;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    fn child(case: &str) -> std::process::Output {
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "delegated_memory::tests::isolated_child",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("SAFE_CEX_MEMORY_TEST_CASE", case)
            .output()
            .unwrap()
    }

    fn succeeds(case: &str) -> std::process::Output {
        let output = child(case);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    #[cfg(target_os = "linux")]
    fn dumpable() -> libc::c_int {
        // SAFETY: scalar query with correctly sized variadic arguments, no pointers.
        unsafe {
            libc::prctl(
                libc::PR_GET_DUMPABLE,
                0 as libc::c_ulong,
                0 as libc::c_ulong,
                0 as libc::c_ulong,
                0 as libc::c_ulong,
            )
        }
    }

    #[test]
    fn delegated_preparation_sets_process_posture() {
        succeeds("posture");
    }

    #[test]
    fn candidates_and_residents_wipe_before_release_on_success_and_error() {
        succeeds("copies");
    }

    #[test]
    fn unavailable_mlock_warns_without_refusing_a_copy() {
        let output = succeeds("unlocked");
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert_eq!(
            stderr
                .matches("DELEGATED memory locking unavailable; continuing")
                .count(),
            1
        );
        assert!(!stderr.contains("synthetic-key"));
    }

    #[test]
    fn venue_declaration_controls_memory_preparation() {
        succeeds("venue-opt-in");
    }

    #[test]
    fn disabled_venues_leave_process_unchanged() {
        succeeds("disabled");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn independent_page_locks_survive_candidate_release() {
        succeeds("locked");
    }

    #[test]
    fn isolated_child() {
        let Ok(case) = std::env::var("SAFE_CEX_MEMORY_TEST_CASE") else {
            return;
        };
        #[cfg(unix)]
        {
            // SAFETY: test reads limits into its own initialized values.
            let limits = || unsafe {
                let mut limit = libc::rlimit {
                    rlim_cur: 0,
                    rlim_max: 0,
                };
                assert_eq!(libc::getrlimit(libc::RLIMIT_CORE, &mut limit), 0);
                (limit.rlim_cur, limit.rlim_max)
            };
            if case == "venue-opt-in" {
                for venue in ["binance", "bybitspot"] {
                    assert!(DelegatedMemory::prepare(venue, false).is_err());
                    assert!(DelegatedMemory::prepare(venue, true).is_ok());
                }
                return;
            }
            if case == "disabled" {
                let before = limits();
                #[cfg(target_os = "linux")]
                let before_dumpable = dumpable();
                for (venue, enabled) in [("bybit", false), ("binance", false), ("bybitspot", false)]
                {
                    assert!(DelegatedMemory::prepare(venue, enabled).is_err());
                }
                assert_eq!(limits(), before);
                #[cfg(target_os = "linux")]
                assert_eq!(dumpable(), before_dumpable);
                return;
            }
            if case == "unlocked" {
                let limit = libc::rlimit {
                    rlim_cur: 0,
                    rlim_max: 0,
                };
                // SAFETY: only this isolated child lowers its own memory lock limit.
                assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_MEMLOCK, &limit) }, 0);
            }
            let memory = DelegatedMemory::prepare("bybit", true).unwrap();
            if case == "posture" {
                assert_eq!(limits(), (0, 0));
                #[cfg(target_os = "linux")]
                assert_eq!(dumpable(), 0);
                return;
            }
            #[cfg(target_os = "linux")]
            if case == "locked" {
                let locked_kib = || {
                    std::fs::read_to_string("/proc/self/status")
                        .unwrap()
                        .lines()
                        .find_map(|line| {
                            line.strip_prefix("VmLck:")
                                .and_then(|value| value.split_whitespace().next())
                                .map(|value| value.parse::<usize>().unwrap())
                        })
                        .unwrap()
                };
                let before = locked_kib();
                let candidate = memory.copy(b"synthetic-key".to_vec()).unwrap();
                let resident = candidate.try_clone().unwrap();
                if candidate.is_locked() && resident.is_locked() {
                    let page_kib = candidate.mapped_len / 1024;
                    assert_eq!(locked_kib(), before + 2 * page_kib);
                    drop(candidate);
                    assert_eq!(locked_kib(), before + page_kib);
                    drop(resident);
                    assert_eq!(locked_kib(), before);
                    println!("independent page locks verified");
                }
                return;
            }
            if case == "unlocked" {
                for _ in 0..2 {
                    let copy = memory.copy(b"synthetic-key".to_vec()).unwrap();
                    assert!(!copy.is_locked());
                    assert!(copy.as_bytes() == b"synthetic-key");
                }
                return;
            }
            let erased = Arc::new(AtomicUsize::new(0));
            let watch = |copy: &mut SecretCopy| {
                let erased = erased.clone();
                copy.before_release = Some(Box::new(move |bytes| {
                    if bytes.iter().all(|byte| *byte == 0) {
                        erased.fetch_add(1, Ordering::SeqCst);
                    }
                }));
            };
            let mut candidate: CandidateCopy = memory.copy(b"synthetic-key".to_vec()).unwrap();
            let mut resident: ResidentCopy = candidate.try_clone().unwrap();
            assert_ne!(candidate.address, resident.address);
            assert!(candidate.as_bytes() == resident.as_bytes());
            assert_eq!(format!("{candidate:?}"), "DELEGATED memory [REDACTED]");
            watch(&mut candidate);
            watch(&mut resident);
            drop(candidate);
            assert!(resident.as_bytes() == b"synthetic-key");
            drop(resident);
            let failed = || -> io::Result<()> {
                let mut candidate = memory.copy(b"synthetic-key".to_vec())?;
                watch(&mut candidate);
                Err(io::ErrorKind::InvalidData.into())
            };
            assert!(failed().is_err());
            assert_eq!(erased.load(Ordering::SeqCst), 3);
            assert!(memory.zeroed(0).unwrap().as_bytes().is_empty());
            assert!(memory.zeroed(usize::MAX).is_err());
            let mut empty = memory.zeroed(4).unwrap();
            empty.as_bytes_mut().copy_from_slice(b"test");
            empty.zeroize();
            assert!(empty.as_bytes().is_empty());
        }
    }
}
