//! Native protected allocations. No Debug, Clone, serialization or key export.
use safe_cex_engine::signing::SignError;
use std::ptr::NonNull;
use zeroize::{Zeroize, Zeroizing};

#[cfg(test)]
type ReleaseCheck = Box<dyn FnOnce(&[u8]) + Send + Sync>;

/// Private material on independent, nondumpable pages, with best-effort locking.
/// Unsupported platforms or failed allocation/dump protection refuse setup
/// instead of falling back to heap. Locking status is non-secret metadata.
///
/// ```compile_fail
/// # use safe_cex_crypto::protected::ProtectedSecret;
/// fn export(secret: &ProtectedSecret) { let _ = secret.as_bytes(); }
/// ```
pub struct ProtectedSecret {
    address: NonNull<u8>,
    len: usize,
    mapped_len: usize,
    locked: bool,
    #[cfg(test)]
    before_release: Option<ReleaseCheck>,
}
// SAFETY: each mapping is exclusively owned; shared access is read-only and
// mutable access/erasure requires &mut self. No raw pointer escapes this module.
unsafe impl Send for ProtectedSecret {}
unsafe impl Sync for ProtectedSecret {}

impl ProtectedSecret {
    #[cfg(test)]
    pub(crate) fn check_release(&mut self, check: impl FnOnce(&[u8]) + Send + Sync + 'static) {
        self.before_release = Some(Box::new(check));
    }
    /// Consume setup input; its temporary heap allocation is erased on all paths.
    pub fn new(bytes: Zeroizing<Vec<u8>>) -> Result<Self, SignError> {
        Self::copy(&bytes)
    }
    pub(crate) fn copy(bytes: &[u8]) -> Result<Self, SignError> {
        #[cfg(target_os = "linux")]
        {
            // SAFETY: sysconf takes no pointer; all mmap lengths are checked.
            let page = usize::try_from(unsafe { libc::sysconf(libc::_SC_PAGESIZE) })
                .ok()
                .filter(|n| *n > 0)
                .ok_or(SignError::Unavailable)?;
            let mapped_len = bytes
                .len()
                .max(1)
                .checked_next_multiple_of(page)
                .filter(|n| *n <= isize::MAX as usize)
                .ok_or(SignError::InvalidRequest)?;
            // SAFETY: fresh private anonymous mapping, no fixed address or file.
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
                return Err(SignError::Unavailable);
            }
            let Some(address) = NonNull::new(raw.cast::<u8>()) else {
                // SAFETY: release the successfully mapped but unusable null base.
                unsafe {
                    libc::munmap(raw, mapped_len);
                }
                return Err(SignError::Unavailable);
            };
            let mut result = Self {
                address,
                len: bytes.len(),
                mapped_len,
                locked: false,
                #[cfg(test)]
                before_release: None,
            };
            // SAFETY: the live independent mapping is page aligned. Keys are
            // copied only after dump protection succeeds; Drop handles failure.
            if unsafe { libc::madvise(raw, mapped_len, libc::MADV_DONTDUMP) } != 0 {
                return Err(SignError::Unavailable);
            }
            result.locked = unsafe { libc::mlock(raw, mapped_len) } == 0;
            // SAFETY: exclusive borrow of a live mapping of at least bytes.len().
            unsafe { std::slice::from_raw_parts_mut(address.as_ptr(), bytes.len()) }
                .copy_from_slice(bytes);
            Ok(result)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = bytes;
            Err(SignError::Unsupported)
        }
    }
    pub(crate) fn as_bytes(&self) -> &[u8] {
        // SAFETY: immutable borrow bounded by this allocation's lifetime/length.
        unsafe { std::slice::from_raw_parts(self.address.as_ptr(), self.len) }
    }
    /// Whether setup locked this allocation. No private material is exposed.
    pub fn memory_locked(&self) -> bool {
        self.locked
    }
    /// Existing Ed25519 primitive; no access to the installed seed is exposed.
    pub fn sign_ed25519(&self, payload: &[u8]) -> Result<[u8; 64], String> {
        crate::ed25519::sign(payload, self.as_bytes())
    }
}
impl Drop for ProtectedSecret {
    fn drop(&mut self) {
        // SAFETY: this exclusively owned live mapping includes all padding;
        // volatile zeroization completes before unlock/unmap or test observation.
        let bytes =
            unsafe { std::slice::from_raw_parts_mut(self.address.as_ptr(), self.mapped_len) };
        bytes.zeroize();
        #[cfg(test)]
        if let Some(check) = self.before_release.take() {
            check(bytes);
        }
        #[cfg(target_os = "linux")]
        // SAFETY: matching allocation and length, erased above exactly once.
        unsafe {
            if self.locked {
                libc::munlock(self.address.as_ptr().cast(), self.mapped_len);
            }
            libc::munmap(self.address.as_ptr().cast(), self.mapped_len);
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    #[test]
    fn drop_erases_entire_mapping_before_release() {
        let observed = Arc::new(AtomicBool::new(false));
        let mut secret = ProtectedSecret::new(Zeroizing::new(vec![0x5a; 35])).unwrap();
        assert_eq!(secret.as_bytes(), &[0x5a; 35]);
        let done = observed.clone();
        secret.before_release = Some(Box::new(move |bytes| {
            assert!(bytes.len() >= 35);
            assert!(bytes.iter().all(|b| *b == 0));
            done.store(true, Ordering::SeqCst);
        }));
        drop(secret);
        assert!(observed.load(Ordering::SeqCst));
    }
    #[test]
    fn native_pages_are_locked_and_excluded_from_core_dumps() {
        let secret = ProtectedSecret::new(Zeroizing::new(vec![7; 32])).unwrap();
        assert!(secret.locked);
        let maps = std::fs::read_to_string("/proc/self/smaps").unwrap();
        let address = secret.address.as_ptr() as usize;
        let mut matched = false;
        for section in maps.split("\nVmFlags:") {
            if matched {
                let flags = section.lines().next().unwrap();
                assert!(flags.split_whitespace().any(|flag| flag == "dd"));
                return;
            }
            matched = section.lines().any(|line| {
                let Some(range) = line.split_whitespace().next() else { return false };
                let Some((lo, hi)) = range.split_once('-') else { return false };
                matches!((usize::from_str_radix(lo,16),usize::from_str_radix(hi,16)),(Ok(lo),Ok(hi)) if address>=lo && address<hi)
            });
        }
        panic!("protected mapping missing from smaps");
    }
    #[test]
    fn unavailable_locking_retains_protected_pages() {
        const CHILD: &str = "SDK_TEST_PROTECTED_MLOCK_REFUSAL";
        if std::env::var_os(CHILD).is_some() {
            let limit = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            // SAFETY: this test-only child lowers only its own memlock limit.
            assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_MEMLOCK, &limit) }, 0);
            let secret = ProtectedSecret::new(Zeroizing::new(vec![7; 32]))
                .expect("published best-effort locking policy keeps protected storage available");
            assert!(!secret.locked);
            assert_eq!(secret.as_bytes(), &[7; 32]);
            return;
        }
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "protected::tests::unavailable_locking_retains_protected_pages",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .status()
            .unwrap();
        assert!(status.success());
    }
}
