use std::fmt::Debug;

use nblf_queue::{MPMCQueue, Queue};
use shared_memory::{Shmem, ShmemConf};

use crate::bitmap::EdgeMapView;

const TOKENS_PER_WORKER: usize = 2;

pub struct SharedMemHandle {
    token_queue: Queue<Box<IPCToken>>,
    pub shmem_size: usize,
}

impl SharedMemHandle {
    pub fn new(n_workers: usize, max_edges: usize) -> Self {
        println!(
            "creating {} tokens with size {}",
            n_workers * TOKENS_PER_WORKER,
            max_edges
        );
        let queue = Queue::new(n_workers * TOKENS_PER_WORKER);

        for i in 0..n_workers * TOKENS_PER_WORKER {
            _ = queue.push(Box::new(IPCToken::new(i, max_edges)));
        }

        Self {
            token_queue: queue,
            shmem_size: max_edges,
        }
    }

    pub fn pop(&self) -> Option<Box<IPCToken>> {
        self.token_queue.pop()
    }

    #[allow(clippy::result_large_err)]
    pub fn push(&self, token: Box<IPCToken>) -> Result<(), Box<IPCToken>> {
        self.token_queue.push(token)
    }
}

impl Debug for SharedMemHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedMemHandle").finish()
    }
}

pub struct IPCToken {
    path: String,
    shmem: Shmem,
    id: usize,
}

impl IPCToken {
    fn new(id: usize, size: usize) -> Self {
        let path = format!("/fuzzer_shm_{}", id);
        let shmem = ShmemConf::new()
            .os_id(&path)
            .size(size)
            .create()
            .expect("could not create shmem");

        Self { path, shmem, id }
    }

    #[allow(dead_code)]
    fn as_mut_slice(&mut self) -> &mut [u8] {
        // SAFETY: May only be called via an exclusive borrow. Since this struct is !Copy and !Clone, only one exclusive borrow may exist at a time
        unsafe { std::slice::from_raw_parts_mut(self.shmem.as_ptr(), self.shmem.len()) }
    }

    fn as_slice(&self) -> &[u8] {
        // SAFETY: concurrent read only access is fine
        unsafe { std::slice::from_raw_parts(self.shmem.as_ptr(), self.shmem.len()) }
    }

    pub fn as_edge_map<'a>(&'a self) -> EdgeMapView<'a> {
        self.as_slice().into()
    }

    pub fn get_path(&self) -> &str {
        &self.path
    }

    pub fn id(&self) -> usize {
        self.id
    }
}

impl Drop for IPCToken {
    fn drop(&mut self) {}
}

impl Debug for IPCToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IPCToken")
            .field("path", &self.path)
            .finish()
    }
}

// SAFETY: the shared memory behind the *mut u8 may only be accessed via an exclusive borrow
unsafe impl Send for IPCToken {}
// SAFETY: the shared memory behind the *mut u8 may only be accessed via an exclusive borrow
unsafe impl Sync for IPCToken {}
