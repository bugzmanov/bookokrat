#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferMode {
    SharedMemory,
    Chunked,
}
