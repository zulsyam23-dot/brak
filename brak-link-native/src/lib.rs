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

impl NativeLinker {
    /// BUG-H02: build a real shared library. Currently only the PE (Windows)
    /// path is implemented — IMAGE_FILE_DLL + export directory.
    pub fn link_shared(&self, objects: &[ObjectFile], base_addr: u64) -> Result<LinkerOutput> {
        let force = std::env::var("BRK_LINK_FORMAT").ok();
        let is_windows_like = match force.as_deref() {
            Some("pe") => true,
            Some("elf") | Some("macho") => false,
            _ => cfg!(target_os = "windows"),
        };
        if is_windows_like {
            pe::link_pe_shared(objects, base_addr)
        } else {
            Err(
                "shared libraries are only supported for PE (Windows) targets \
                 right now; ELF ET_DYN / Mach-O dylib are not implemented yet"
                    .into(),
            )
        }
    }
}
