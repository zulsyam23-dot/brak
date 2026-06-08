use brak_core::Result;

/// Object file produced by a codegen backend
#[derive(Debug, Clone)]
pub struct ObjectFile {
    pub name: String,
    pub data: Vec<u8>,
}

/// Linker output: an executable binary
#[derive(Debug, Clone)]
pub struct LinkerOutput {
    pub data: Vec<u8>,
    pub format: &'static str,
}

pub trait LinkerBackend: Send + Sync {
    fn name(&self) -> &'static str;

    /// Link one or more object files into an executable.
    fn link(
        &self,
        objects: &[ObjectFile],
        entry: &str,
        base_addr: u64,
    ) -> Result<LinkerOutput>;
}
