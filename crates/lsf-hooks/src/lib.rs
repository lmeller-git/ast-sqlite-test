#![allow(clippy::missing_safety_doc)]
use std::{env, sync::OnceLock};

use shared_memory::{Shmem, ShmemConf};

static mut SHMEM_MAP: *mut u8 = std::ptr::null_mut();
static SHMEM_GUARD: OnceLock<StoreShmem> = OnceLock::new();

struct StoreShmem {
    _shmem: Shmem,
}

unsafe impl Send for StoreShmem {}
unsafe impl Sync for StoreShmem {}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __sanitizer_cov_trace_pc_guard_init(start: *mut u32, stop: *mut u32) {
    if start == stop || start.is_null() {
        return;
    }

    let count = unsafe { stop.offset_from(start) } as usize;
    let guards = unsafe { std::slice::from_raw_parts_mut(start, count) };

    let mut edges: u32 = 1;

    for guard in guards.iter_mut() {
        *guard = edges;
        edges += 1;
    }

    if env::var("FUZZER_INIT").is_ok() {
        println!("FUZZER_INIT: max edges = {edges}");
    } else {
        let path = env::var("FUZZER_SHMEM_PATH").expect("no memory for pc_guard provided");
        let shmem = ShmemConf::new()
            .os_id(path)
            .open()
            .expect("could not open shmem");
        unsafe {
            SHMEM_MAP = shmem.as_ptr();
        }
        _ = SHMEM_GUARD.set(StoreShmem { _shmem: shmem });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __sanitizer_cov_trace_pc_guard(guard: *mut u32) {
    let v = unsafe { *guard };
    if v == 0 {
        return;
    }

    unsafe {
        if !SHMEM_MAP.is_null() {
            *SHMEM_MAP.add(v as usize) = 1;
        }
    }
}
