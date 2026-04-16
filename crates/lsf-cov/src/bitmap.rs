// TODO maybe change this to bitvec/bitslice later to save memory

static mut TOTAL_FOUND: usize = 0;

#[derive(Debug)]
pub struct EdgeMap {
    raw_map: Vec<u8>,
}

impl EdgeMap {
    pub fn new(max_edges: usize) -> Self {
        // one byte per edge
        Self {
            raw_map: vec![0; max_edges],
        }
    }

    pub fn update<'a>(&mut self, other: EdgeMapView<'a>) -> usize {
        debug_assert_eq!(
            self.raw_map.len(),
            other.raw_view.len(),
            "Map sizes must match"
        );

        let mut new_edges = 0;

        // we can look at 8 bytes == 1 qword at a time
        let their_chunks = other.raw_view.chunks_exact(8);
        let our_chunks = self.raw_map.chunks_exact_mut(8);

        for (their_chunk, our_chunk) in their_chunks.zip(our_chunks) {
            let their_val = u64::from_ne_bytes(their_chunk.try_into().unwrap());

            // no edges found here
            if their_val == 0 {
                continue;
            }

            let our_val = u64::from_ne_bytes(our_chunk.try_into().unwrap());

            if (their_val & !our_val) > 0 {
                // new edge
                for i in 0..8 {
                    if their_chunk[i] > 0 && our_chunk[i] == 0 {
                        new_edges += 1;
                        our_chunk[i] = 1;
                    }
                }
            }
        }

        let chunk_len = self.raw_map.len() / 8 * 8;
        for i in chunk_len..self.raw_map.len() {
            if other.raw_view[i] > 0 && self.raw_map[i] == 0 {
                new_edges += 1;
                self.raw_map[i] = 1;
            }
        }

        unsafe {
            TOTAL_FOUND += new_edges;
        }

        if new_edges > 0 {
            println!(
                "Total coverage so far: {:.3}%",
                unsafe { TOTAL_FOUND } as f64 / self.raw_map.len() as f64 * 100.0
            )
        }

        new_edges
    }
}

pub struct EdgeMapView<'a> {
    raw_view: &'a [u8],
}

impl<'a> EdgeMapView<'a> {}

impl<'a> From<&'a [u8]> for EdgeMapView<'a> {
    fn from(value: &'a [u8]) -> Self {
        Self { raw_view: value }
    }
}
