#![allow(clippy::missing_safety_doc, clippy::manual_c_str_literals)]
use core::{
    ffi::CStr,
    ptr::null_mut,
    sync::atomic::{AtomicBool, AtomicPtr, AtomicU32},
};
use std::sync::OnceLock;

use shared_memory::{Shmem, ShmemConf};

unsafe extern "C" {
    fn getenv(name: *const i8) -> *const i8;
    fn printf(format: *const i8, ...) -> i32;
}

static SHMEM_MAP: AtomicPtr<u8> = AtomicPtr::new(null_mut());
static EDGES: AtomicU32 = AtomicU32::new(0);
static NEED_INIT: AtomicBool = AtomicBool::new(true);
static SHMEM_STORAGE: OnceLock<StoreShmem> = OnceLock::new();

struct StoreShmem {
    _shmem: Shmem,
}

unsafe impl Send for StoreShmem {}
unsafe impl Sync for StoreShmem {}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __sanitizer_cov_trace_pc_guard_init(start: *mut u32, stop: *mut u32) {
    if start == stop || start.is_null() || unsafe { *start != 0 } {
        return;
    }

    let count = unsafe { stop.offset_from(start) } as usize;
    let guards = unsafe { core::slice::from_raw_parts_mut(start, count) };

    for guard in guards.iter_mut() {
        let id = EDGES.fetch_add(1, core::sync::atomic::Ordering::AcqRel) + 1;
        *guard = id;
    }

    let init_key = b"FUZZER_INIT\0".as_ptr() as *const i8;
    let is_init_mode = !unsafe { getenv(init_key) }.is_null();

    if is_init_mode {
        let fmt = b"FUZZER_INIT: max edges = %u\n\0".as_ptr() as *const i8;
        unsafe { printf(fmt, EDGES.load(core::sync::atomic::Ordering::Acquire)) };
    } else if NEED_INIT.swap(false, core::sync::atomic::Ordering::AcqRel) {
        let path_key = b"FUZZER_SHMEM_PATH\0".as_ptr() as *const i8;
        let path_ptr = unsafe { getenv(path_key) };

        if path_ptr.is_null() {
            return;
        }
        if let Ok(path) = unsafe { CStr::from_ptr(path_ptr) }.to_str() {
            let shmem = ShmemConf::new()
                .os_id(path)
                .open()
                .expect("could not open shmem");
            SHMEM_MAP.store(shmem.as_ptr(), core::sync::atomic::Ordering::Release);
            _ = SHMEM_STORAGE.set(StoreShmem { _shmem: shmem });
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __sanitizer_cov_trace_pc_guard(guard: *mut u32) {
    if guard.is_null() {
        return;
    }
    let v = unsafe { *guard };
    if v == 0 {
        return;
    }

    let map = SHMEM_MAP.load(core::sync::atomic::Ordering::Relaxed);

    if map.is_null() {
        return;
    }

    unsafe {
        *map.add(v as usize - 1) = 1;
    }
}
