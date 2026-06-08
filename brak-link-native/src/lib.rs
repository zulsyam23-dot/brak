pub mod elf;
pub mod macho;
pub mod parse;
pub mod pe;

use brak_core::Result;
use brak_link_traits::{LinkerBackend, LinkerOutput, ObjectFile};

pub struct NativeLinker;

impl LinkerBackend for NativeLinker {
    fn name(&self) -> &'static str {
        if cfg!(target_os = "windows") {
            "native-pe"
        } else if cfg!(target_os = "macos") {
            "native-macho"
        } else {
            "native-elf"
        }
    }

    fn link(
        &self,
        objects: &[ObjectFile],
        entry: &str,
        base_addr: u64,
    ) -> Result<LinkerOutput> {
        // Auto-detect target format based on host OS.
        // Override via environment variable BRK_LINK_FORMAT=elf|pe|macho.
        let force = std::env::var("BRK_LINK_FORMAT").ok();
        match force.as_deref() {
            Some("elf") => elf::link_elf(objects, entry, base_addr),
            Some("pe") => pe::link_pe(objects, entry, base_addr),
            Some("macho") => macho::link_macho(objects, entry, base_addr),
            Some(_) => {
                if cfg!(target_os = "windows") {
                    pe::link_pe(objects, entry, base_addr)
                } else if cfg!(target_os = "macos") {
                    macho::link_macho(objects, entry, base_addr)
                } else {
                    elf::link_elf(objects, entry, base_addr)
                }
            }
            None => {
                if cfg!(target_os = "windows") {
                    pe::link_pe(objects, entry, base_addr)
                } else if cfg!(target_os = "macos") {
                    macho::link_macho(objects, entry, base_addr)
                } else {
                    elf::link_elf(objects, entry, base_addr)
                }
            }
        }
    }
}
