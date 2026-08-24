use brak_core::Result;
use std::io::Write;
use brak_link_native::parse::{parse_coff, parse_elf, parse_macho};

/// Names of DEFINED global symbols in a member object, detected by magic.
/// Unparseable members contribute nothing (the index is an optimization —
/// the linker still resolves symbols by scanning members).
fn defined_global_symbols(data: &[u8]) -> Vec<String> {
    let parsed = if data.len() >= 4 && data[0..4] == [0x7f, b'E', b'L', b'F'] {
        parse_elf(data).ok()
    } else if data.len() >= 4 && u32::from_le_bytes([data[0], data[1], data[2], data[3]]) == 0xFEEDFACF {
        parse_macho(data).ok()
    } else if data.len() >= 2 && u16::from_le_bytes([data[0], data[1]]) == 0x8664 {
        parse_coff(data).ok()
    } else {
        None
    };
    match parsed {
        Some(p) => p.symbols.iter()
            .filter(|s| !s.name.is_empty() && s.st_shndx != 0 && (s.st_info >> 4) == 1 /* STB_GLOBAL */)
            .map(|s| s.name.clone())
            .collect(),
        None => Vec::new(),
    }
}

pub struct ArchiveEntry {
    pub name: String,
    pub data: Vec<u8>,
}

pub enum ArchiveFormat {
    Unix,    // .a
    Windows, // .lib (actually very similar to Unix but with different symbol table)
}

pub struct ArchiveWriter {
    pub format: ArchiveFormat,
    pub entries: Vec<ArchiveEntry>,
}

impl ArchiveWriter {
    pub fn new(format: ArchiveFormat) -> Self {
        Self {
            format,
            entries: Vec::new(),
        }
    }

    pub fn add_entry(&mut self, name: String, data: Vec<u8>) {
        self.entries.push(ArchiveEntry { name, data });
    }

    pub fn write(&self) -> Result<Vec<u8>> {
        // BUG-M09: the index previously derived symbol names from FILENAMES and
        // wrote offset 0 placeholders, making it unusable. Now every member is
        // parsed as an object file (ELF/COFF/Mach-O) and its DEFINED global
        // symbols are indexed with real member-header offsets.
        //
        // GNU "/" layout: [count BE u32][offsets BE u32 × N][names NUL-terminated]
        let member_syms: Vec<(Vec<String>, usize)> = self.entries.iter()
            .map(|e| (defined_global_symbols(&e.data), e.data.len()))
            .collect();

        let total_symbols: usize = member_syms.iter().map(|(s, _)| s.len()).sum();
        let name_table_len: usize =
            member_syms.iter().flat_map(|(s, _)| s.iter()).map(|n| n.len() + 1).sum();
        let sym_body_len = 4 + total_symbols * 4 + name_table_len;
        let first_member_offset = 8 + 60 + sym_body_len + (sym_body_len % 2);

        let mut offsets: Vec<u32> = Vec::new();
        let mut names: Vec<&String> = Vec::new();
        let mut cursor = first_member_offset as u32;
        for ((syms, _), entry) in member_syms.iter().zip(&self.entries) {
            for n in syms {
                offsets.push(cursor);
                names.push(n);
            }
            cursor += 60 + entry.data.len() as u32 + (entry.data.len() as u32 % 2);
        }

        let mut buf = Vec::new();
        buf.write_all(b"!<arch>\n").unwrap();
        if total_symbols > 0 {
            let mut sym_buf = Vec::new();
            sym_buf.extend_from_slice(&(total_symbols as u32).to_be_bytes());
            for off in &offsets {
                sym_buf.extend_from_slice(&off.to_be_bytes());
            }
            for name in &names {
                sym_buf.extend_from_slice(name.as_bytes());
                sym_buf.push(0);
            }
            debug_assert_eq!(sym_buf.len(), sym_body_len);
            self.write_entry(&mut buf, "/", &sym_buf)?;
        }

        for entry in &self.entries {
            self.write_entry(&mut buf, &entry.name, &entry.data)?;
        }
        Ok(buf)
    }

    fn write_entry(&self, buf: &mut Vec<u8>, name: &str, data: &[u8]) -> Result<()> {
        // File header (60 bytes)
        let mut header = [b' '; 60];
        
        let name_bytes = name.as_bytes();
        let name_len = name_bytes.len().min(16);
        header[0..name_len].copy_from_slice(&name_bytes[0..name_len]);

        // Timestamp (dummy 0)
        header[16..28].copy_from_slice(format!("{:<12}", 0).as_bytes());
        // Owner/Group (dummy 0)
        header[28..34].copy_from_slice(format!("{:<6}", 0).as_bytes());
        header[34..40].copy_from_slice(format!("{:<6}", 0).as_bytes());
        // Mode (dummy 644 octal)
        header[40..48].copy_from_slice(format!("{:<8}", "100644").as_bytes());
        // Size
        header[48..58].copy_from_slice(format!("{:<10}", data.len()).as_bytes());
        // Ending magic
        header[58..60].copy_from_slice(b"`\n");

        buf.write_all(&header).unwrap();
        buf.write_all(data).unwrap();

        // Padding to even byte
        if data.len() % 2 != 0 {
            buf.push(b'\n');
        }
        Ok(())
    }
}

/// BUG-H03: brak-tool previously handed raw archive bytes straight to the
/// ELF/COFF object parsers, so linking `.a`/`.lib` inputs ALWAYS failed.
/// This reader extracts the member objects (skipping the `/` symbol index and
/// `//` long-name table) so they can be linked individually.
pub fn parse_archive(data: &[u8]) -> Result<Vec<ArchiveEntry>> {
    const MAGIC: &[u8] = b"!<arch>\n";
    if data.len() < MAGIC.len() + 60 || &data[0..MAGIC.len()] != MAGIC {
        return Err("not a valid archive (missing !<arch> magic)".into());
    }

    let mut entries = Vec::new();
    let mut pos = MAGIC.len();
    while pos + 60 <= data.len() {
        let header = &data[pos..pos + 60];
        if &header[58..60] != b"`\n" {
            return Err(format!("corrupt archive member header at offset {pos}").into());
        }
        let raw_name = String::from_utf8_lossy(&header[0..16]).trim_end().to_string();
        let size_str = String::from_utf8_lossy(&header[48..58]);
        let size: usize = size_str.trim().parse().map_err(|_| {
            format!("invalid member size '{size_str}' at offset {pos}")
        })?;
        pos += 60;

        if pos + size > data.len() {
            return Err("archive truncated".into());
        }
        let content = data[pos..pos + size].to_vec();
        pos += size;
        if pos % 2 != 0 { pos += 1; } // members are 2-byte aligned

        // Skip special members: '/' (GNU symbol index), '//' (long names),
        // '/SYM64/' (64-bit index). Real object members end in ".o"/".obj"
        // or at least don't start with '/'.
        if raw_name == "/" || raw_name == "//" || raw_name == "/SYM64/" {
            continue;
        }
        let name = raw_name.trim_end_matches('/').to_string(); // GNU trailing slash
        entries.push(ArchiveEntry { name, data: content });
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_roundtrip() {
        let mut writer = ArchiveWriter::new(ArchiveFormat::Unix);
        writer.add_entry("a.o".to_string(), vec![1, 2, 3, 4]);
        writer.add_entry("b.o".to_string(), vec![5, 6, 7]); // odd size → padding
        let data = writer.write().unwrap();

        let entries = parse_archive(&data).unwrap();
        assert_eq!(entries.len(), 2, "symbol index must be skipped");
        assert_eq!(entries[0].name, "a.o");
        assert_eq!(entries[0].data, vec![1, 2, 3, 4]);
        assert_eq!(entries[1].name, "b.o");
        assert_eq!(entries[1].data, vec![5, 6, 7]);
    }

    /// BUG-M09 regression: a real object member's defined globals must appear
    /// in the "/" index with correct member offsets.
    #[test]
    fn test_symbol_index_from_real_objects() {
        use brak_codegen_obj::ObjBackend;
        use brak_codegen_traits::CodegenBackend;
        use brak_frontend::lexer::{AsciiLexer, BrakLexer};
        use brak_frontend::parser::Parser as BrakParser;
        use brak_ir_hir::lower::HirLower;
        use brak_ir_hir::typeck::TypeChecker;
        use brak_ir_mir::lower::MirLower;
        use brak_ir_lir::lower::LirLower;

        let src = "fn add(a: i32, b: i32) -> i32 { return a + b; }";
        let sm = brak_core::SourceMap::new("t.brk", src);
        let tokens = AsciiLexer::new().lex(&sm);
        let ast = BrakParser::new().parse(&tokens).unwrap();
        let hir = HirLower::new().lower(ast).unwrap();
        TypeChecker::new().check(&hir).unwrap();
        let mir = MirLower::new().lower(hir).unwrap();
        let obj = ObjBackend::default().emit(&LirLower::new().lower(mir)).unwrap();

        let mut writer = ArchiveWriter::new(ArchiveFormat::Unix);
        writer.add_entry("add.o".to_string(), obj);
        let data = writer.write().unwrap();
        // The "/" member must exist and mention "add".
        assert!(data.windows(16).any(|w| w == b"/               "), "symbol index present");

        // Stronger check: parse the index back.
        let body_start = data.windows(16).position(|w| w == b"/               ").unwrap() + 60;
        let count = u32::from_be_bytes(data[body_start..body_start+4].try_into().unwrap());
        assert!(count >= 1, "at least 'add' indexed, got {count}");
        let off = u32::from_be_bytes(data[body_start+4..body_start+8].try_into().unwrap());
        // offset must point at the add.o member header
        assert_eq!(&data[off as usize..off as usize + 5], b"add.o", "index offset points at member header");
    }

    #[test]
    fn test_basic_archive() {
        let mut writer = ArchiveWriter::new(ArchiveFormat::Unix);
        writer.add_entry("test.o".to_string(), vec![1, 2, 3, 4]);
        writer.add_entry("other.o".to_string(), vec![5, 6, 7]);
        
        let data = writer.write().unwrap();
        assert!(data.starts_with(b"!<arch>\n"));

        // Members are raw bytes (not parseable objects) → no defined symbols
        // → no "/" symbol index is emitted (BUG-M09: index only for real ones).
        assert!(!data.windows(16).any(|w| w == b"/               "));

        let has_test = data.windows(16).any(|w| w == b"test.o          ");
        let has_other = data.windows(16).any(|w| w == b"other.o         ");

        assert!(has_test);
        assert!(has_other);
    }
}
