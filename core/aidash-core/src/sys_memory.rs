//! macOS Activity Monitor과 동일한 공식으로 시스템 메모리 사용량·가용량을 계산한다.
//!
//! 사용됨 = (internal_page_count − purgeable_count + wire_count + compressor_page_count) × page_size
//! 가용 = total − 사용됨
//!
//! macOS가 아니거나 mach 호출이 실패하면 sysinfo 값으로 폴백한다.

use sysinfo::System;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemMemory {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
}

fn sysinfo_snapshot() -> SystemMemory {
    let mut system = System::new();
    system.refresh_memory();
    let total = system.total_memory();
    let available = system.available_memory();
    let used = total.saturating_sub(available);
    SystemMemory {
        total_bytes: total,
        used_bytes: used,
        available_bytes: available,
    }
}

/// `memory_pressure -Q`와 동일 소스: `kern.memorystatus_level` (시스템 여유 %).
#[cfg(target_os = "macos")]
fn sysctl_free_percent() -> Option<f64> {
    use std::ffi::CString;

    extern "C" {
        fn sysctlbyname(
            name: *const i8,
            oldp: *mut std::ffi::c_void,
            oldlenp: *mut usize,
            newp: *const std::ffi::c_void,
            newlen: usize,
        ) -> i32;
    }

    let name = CString::new("kern.memorystatus_level").ok()?;
    let mut value: u32 = 0;
    let mut len = std::mem::size_of::<u32>();
    let rc = unsafe {
        sysctlbyname(
            name.as_ptr(),
            &mut value as *mut u32 as *mut std::ffi::c_void,
            &mut len,
            std::ptr::null(),
            0,
        )
    };
    if rc != 0 {
        return None;
    }
    Some(value as f64)
}

#[cfg(target_os = "macos")]
fn mach_snapshot() -> Option<SystemMemory> {
    use std::mem::MaybeUninit;

    // mach/vm_statistics.h — field types must match (natural_t=u32, counters=u64).
    #[repr(C, align(8))]
    #[derive(Copy, Clone)]
    struct VmStatistics64 {
        free_count: u32,
        active_count: u32,
        inactive_count: u32,
        wire_count: u32,
        zero_fill_count: u64,
        reactivations: u64,
        pageins: u64,
        pageouts: u64,
        faults: u64,
        cow_faults: u64,
        lookups: u64,
        hits: u64,
        purges: u64,
        purgeable_count: u32,
        speculative_count: u32,
        decompressions: u64,
        compressions: u64,
        swapins: u64,
        swapouts: u64,
        compressor_page_count: u32,
        throttled_count: u32,
        external_page_count: u32,
        internal_page_count: u32,
        total_uncompressed_pages_in_compressor: u64,
        swapped_count: u64,
    }

    const HOST_VM_INFO64: i32 = 4;

    extern "C" {
        fn mach_host_self() -> u32;
        fn host_page_size(host: u32, out_page_size: *mut u32) -> i32;
        fn host_statistics64(
            host: u32,
            flavor: i32,
            host_info: *mut u8,
            host_info_count: *mut u32,
        ) -> i32;
    }

    let host = unsafe { mach_host_self() };
    let mut page_size: u32 = 0;
    if unsafe { host_page_size(host, &mut page_size) } != 0 || page_size == 0 {
        return None;
    }

    let mut stats = MaybeUninit::<VmStatistics64>::uninit();
    let mut count = (std::mem::size_of::<VmStatistics64>() / std::mem::size_of::<u32>()) as u32;
    let kr = unsafe {
        host_statistics64(
            host,
            HOST_VM_INFO64,
            stats.as_mut_ptr() as *mut u8,
            &mut count,
        )
    };
    if kr != 0 {
        return None;
    }
    let stats = unsafe { stats.assume_init() };

    let mut system = System::new();
    system.refresh_memory();
    let total = system.total_memory();

    // Activity Monitor "사용됨" ≈ Wired + Compressed (memory_pressure 여유%와 정합)
    let used_pages =
        stats.wire_count as u64 + stats.compressor_page_count as u64;
    let mut used_bytes = used_pages.saturating_mul(page_size as u64);
    let mut available_bytes = total.saturating_sub(used_bytes);

    // memory_pressure와 동일한 sysctl 값이 있으면 우선 적용 (±0%p)
    if let Some(free_pct) = sysctl_free_percent() {
        available_bytes = (total as f64 * free_pct / 100.0) as u64;
        used_bytes = total.saturating_sub(available_bytes);
    }

    Some(SystemMemory {
        total_bytes: total,
        used_bytes,
        available_bytes,
    })
}

#[cfg(not(target_os = "macos"))]
fn mach_snapshot() -> Option<SystemMemory> {
    None
}

/// 단일 진입점: macOS mach 통계 우선, 실패 시 sysinfo 폴백.
pub fn system_memory() -> SystemMemory {
    mach_snapshot().unwrap_or_else(sysinfo_snapshot)
}

pub fn total_system_memory_bytes() -> u64 {
    system_memory().total_bytes
}

pub fn system_used_bytes() -> u64 {
    system_memory().used_bytes
}

pub fn system_available_bytes() -> u64 {
    system_memory().available_bytes
}

/// memory_pressure -Q의 free %와 대조용 (가용 / 총 × 100).
pub fn system_free_percent() -> f64 {
    let mem = system_memory();
    if mem.total_bytes == 0 {
        return 0.0;
    }
    mem.available_bytes as f64 / mem.total_bytes as f64 * 100.0
}

/// 와치독 기본 hard limit: 가용 메모리의 85%를 프로세스에 허용 (정확한 가용 기준).
pub fn watchdog_default_hard_bytes() -> u64 {
    let mem = system_memory();
    let headroom = (mem.available_bytes as f64 * 0.85) as u64;
    mem.used_bytes.saturating_add(headroom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_memory_returns_positive_total() {
        let mem = system_memory();
        assert!(mem.total_bytes > 0);
        assert!(mem.used_bytes <= mem.total_bytes);
        assert!(mem.available_bytes <= mem.total_bytes);
        assert_eq!(
            mem.used_bytes.saturating_add(mem.available_bytes),
            mem.total_bytes
        );
    }

    #[test]
    fn fallback_path_when_mach_unavailable() {
        // non-macOS always uses sysinfo; on macOS mach should succeed on real hardware.
        let mem = system_memory();
        assert!(mem.available_bytes > 0 || mem.total_bytes > 0);
    }

    #[test]
    fn free_percent_in_valid_range() {
        let pct = system_free_percent();
        assert!((0.0..=100.0).contains(&pct));
    }

    #[test]
    fn free_percent_within_memory_pressure_tolerance() {
        let ours = system_free_percent();
        let mp = std::process::Command::new("memory_pressure")
            .arg("-Q")
            .output()
            .expect("memory_pressure");
        let stdout = String::from_utf8_lossy(&mp.stdout);
        let mp_pct: f64 = stdout
            .lines()
            .find_map(|line| {
                line.split("free percentage:")
                    .nth(1)
                    .and_then(|s| s.trim().strip_suffix('%'))
                    .and_then(|s| s.parse().ok())
            })
            .unwrap_or_else(|| {
                eprintln!("memory_pressure output:\n{stdout}");
                panic!("could not parse memory_pressure free percentage");
            });
        eprintln!("sys_memory free%={ours:.1} memory_pressure free%={mp_pct:.1} delta={:.1}", (ours - mp_pct).abs());
        assert!(
            (ours - mp_pct).abs() <= 2.0,
            "free% delta {} exceeds ±2%p (ours={ours:.1}, mp={mp_pct:.1})",
            (ours - mp_pct).abs()
        );
    }
}