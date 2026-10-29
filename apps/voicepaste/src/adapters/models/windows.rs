//! Windows receipt-v1 timestamps. Keep the saved cache format unchanged; future
//! OS adapters must invalidate/reverify incompatible receipts, not reinterpret it.
use std::{fs::Metadata, os::windows::fs::MetadataExt};

pub(super) fn times(metadata: &Metadata) -> (u64, u64) {
    (metadata.creation_time(), metadata.last_write_time())
}
