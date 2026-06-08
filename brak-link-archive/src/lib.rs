use brak_core::Result;
use std::io::Write;

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
        let mut buf = Vec::new();
        
        // Global header
        buf.write_all(b"!<arch>\n").unwrap();

        // ── SYMBOL TABLE (Entry '/') ──────────────────────────
        // For simplicity, we create a basic GNU-style symbol table
        // This is usually the first entry.
        let mut sym_names = Vec::new();
        for entry in &self.entries {
            // Simplified: we'd normally parse the .o file here to find global symbols
            // For now, let's assume the filename (without .o) is the symbol it provides
            let sym = entry.name.strip_suffix(".o").unwrap_or(&entry.name);
            sym_names.push(sym.to_string());
        }

        let mut sym_buf = Vec::new();
        // Number of symbols (BE 32-bit)
        sym_buf.extend_from_slice(&(sym_names.len() as u32).to_be_bytes());
        // Offsets for each symbol (placeholder for now)
        for _ in 0..sym_names.len() {
            sym_buf.extend_from_slice(&0u32.to_be_bytes());
        }
        // String table
        for name in &sym_names {
            sym_buf.extend_from_slice(name.as_bytes());
            sym_buf.push(0);
        }

        self.write_entry(&mut buf, "/", &sym_buf)?;

        // ── FILE ENTRIES ─────────────────────────────────────
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_archive() {
        let mut writer = ArchiveWriter::new(ArchiveFormat::Unix);
        writer.add_entry("test.o".to_string(), vec![1, 2, 3, 4]);
        writer.add_entry("other.o".to_string(), vec![5, 6, 7]);
        
        let data = writer.write().unwrap();
        assert!(data.starts_with(b"!<arch>\n"));
        
        // Entry '/' (Symbol Table) should be present
        assert!(data.windows(16).any(|w| w == b"/               "));
        
        let has_test = data.windows(16).any(|w| w == b"test.o          ");
        let has_other = data.windows(16).any(|w| w == b"other.o         ");
        
        assert!(has_test);
        assert!(has_other);
    }
}
