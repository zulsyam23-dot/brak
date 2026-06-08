use crate::x86_64::*;
use crate::codeview;
use brak_ir_lir::lir::LirProgram;

pub fn write_coff(program: &LirProgram) -> Result<Vec<u8>, CodegenError> {
    let mut buf = Vec::new();

    let (text_data, relocs, line_entries) = emit_text(program)?;
    let strtab = build_strtab(program, &relocs);
    let symtab = build_symtab(program, &strtab, &relocs)?;
    let coff_relocs = build_relocs(&relocs, &symtab, &strtab);

    // Build COFF line number entries (6 bytes each)
    let mut line_number_data = Vec::new();
    for entry in &line_entries {
        line_number_data.extend_from_slice(&(entry.offset as u32).to_le_bytes()); // address
        let ln: u16 = if entry.end_sequence { 0 } else { entry.line as u16 };
        line_number_data.extend_from_slice(&ln.to_le_bytes()); // line number
    }
    let line_number_count = line_number_data.len() as u16 / 6;

    // Build CodeView C13 debug data for .debug$S section
    let debug_data = codeview::build_codeview(program, &line_entries, text_data.len());

    let has_debug = !debug_data.is_empty();
    let num_sections: u16 = if has_debug { 2 } else { 1 };

    // ── COFF FILE HEADER (20 bytes) ────────────────────
    buf.extend_from_slice(&0x8664u16.to_le_bytes()); // Machine = AMD64
    buf.extend_from_slice(&num_sections.to_le_bytes()); // NumberOfSections
    buf.extend_from_slice(&0u32.to_le_bytes());      // TimeDateStamp
    let text_raw_off = 20 + (num_sections as u32) * 40;
    let reloc_off = text_raw_off + text_data.len() as u32;
    let line_off = reloc_off + coff_relocs.len() as u32;
    let debug_off = line_off + line_number_data.len() as u32;
    let symtab_off = debug_off + debug_data.len() as u32;

    buf.extend_from_slice(&symtab_off.to_le_bytes()); // PointerToSymbolTable
    buf.extend_from_slice(&(symtab.len() as u32 / 18).to_le_bytes()); // NumberOfSymbols
    buf.extend_from_slice(&0u16.to_le_bytes());      // SizeOfOptionalHeader
    buf.extend_from_slice(&0u16.to_le_bytes());      // Characteristics

    // ── SECTION HEADER (.text) (40 bytes) ──────────────
    buf.extend_from_slice(b".text\0\0\0");
    buf.extend_from_slice(&0u32.to_le_bytes());      // VirtualSize
    buf.extend_from_slice(&0u32.to_le_bytes());      // VirtualAddress
    buf.extend_from_slice(&(text_data.len() as u32).to_le_bytes()); // SizeOfRawData
    buf.extend_from_slice(&text_raw_off.to_le_bytes()); // PointerToRawData
    buf.extend_from_slice(&reloc_off.to_le_bytes());     // PointerToRelocations
    buf.extend_from_slice(&line_off.to_le_bytes());      // PointerToLinenumbers
    buf.extend_from_slice(&(relocs.len() as u16).to_le_bytes()); // NumberOfRelocations
    buf.extend_from_slice(&line_number_count.to_le_bytes());      // NumberOfLinenumbers
    buf.extend_from_slice(&0x60500020u32.to_le_bytes()); // Characteristics (CODE | EXECUTE | READ | ALIGN_16BYTES)

    // ── SECTION HEADER (.debug$S) (40 bytes) ──────────
    if has_debug {
        buf.extend_from_slice(b".debug$S"); // exactly 8 bytes
        buf.extend_from_slice(&0u32.to_le_bytes());      // VirtualSize
        buf.extend_from_slice(&0u32.to_le_bytes());      // VirtualAddress
        buf.extend_from_slice(&(debug_data.len() as u32).to_le_bytes()); // SizeOfRawData
        buf.extend_from_slice(&debug_off.to_le_bytes());   // PointerToRawData
        buf.extend_from_slice(&0u32.to_le_bytes());        // PointerToRelocations
        buf.extend_from_slice(&0u32.to_le_bytes());        // PointerToLinenumbers
        buf.extend_from_slice(&0u16.to_le_bytes());        // NumberOfRelocations
        buf.extend_from_slice(&0u16.to_le_bytes());        // NumberOfLinenumbers
        buf.extend_from_slice(&0x42100040u32.to_le_bytes()); // INITIALIZED_DATA | INFO_DISCARDABLE | MEM_DISCARDABLE | MEM_READ
    }

    // ── .text data ─────────────────────────────────────
    buf.extend_from_slice(&text_data);

    // ── Relocations ────────────────────────────────────
    buf.extend_from_slice(&coff_relocs);

    // ── Line numbers ───────────────────────────────────
    buf.extend_from_slice(&line_number_data);

    // ── .debug$S data ──────────────────────────────────
    if has_debug {
        buf.extend_from_slice(&debug_data);
    }

    // ── Symbol Table ───────────────────────────────────
    buf.extend_from_slice(&symtab);

    // ── String Table ───────────────────────────────────
    buf.extend_from_slice(&(strtab.len() as u32 + 4).to_le_bytes());
    buf.extend_from_slice(strtab.as_bytes());

    Ok(buf)
}

fn build_strtab(program: &LirProgram, relocs: &[Reloc]) -> String {
    let mut s = String::new();
    let mut seen = std::collections::HashSet::new();

    for func in &program.functions {
        if func.name.len() > 8 {
            s.push_str(&func.name);
            s.push('\0');
            seen.insert(func.name.clone());
        }
    }

    for r in relocs {
        if r.target_name.len() > 8 && !seen.contains(&r.target_name) {
            s.push_str(&r.target_name);
            s.push('\0');
            seen.insert(r.target_name.clone());
        }
    }
    s
}

fn build_symtab(program: &LirProgram, _strtab: &str, relocs: &[Reloc]) -> Result<Vec<u8>, CodegenError> {
    let mut buf = Vec::new();
    let mut str_off = 4u32;
    let mut text_off = 0u32;

    let mut func_labels = std::collections::HashMap::new();
    let mut block_labels = std::collections::HashMap::new();

    // 1. Internal functions
    for func in &program.functions {
        let mut entry = [0u8; 18];
        if func.name.len() <= 8 {
            entry[0..func.name.len()].copy_from_slice(func.name.as_bytes());
        } else {
            entry[4..8].copy_from_slice(&str_off.to_le_bytes());
            str_off += func.name.len() as u32 + 1;
        }

        entry[8..12].copy_from_slice(&text_off.to_le_bytes());
        entry[12..14].copy_from_slice(&1i16.to_le_bytes()); // Section 1 (.text)
        entry[14..16].copy_from_slice(&0x20u16.to_le_bytes());
        entry[16] = 2; // EXTERNAL
        buf.extend_from_slice(&entry);

        let mut dummy_lines = Vec::new();
        let (code, _) = emit_function(func, &mut func_labels, &mut block_labels, &mut dummy_lines)?;
        text_off += code.len() as u32;
    }

    // 2. External symbols from relocs
    let mut seen = program.functions.iter().map(|f| f.name.clone()).collect::<std::collections::HashSet<_>>();
    for r in relocs {
        if !seen.contains(&r.target_name) {
            let mut entry = [0u8; 18];
            if r.target_name.len() <= 8 {
                entry[0..r.target_name.len()].copy_from_slice(r.target_name.as_bytes());
            } else {
                entry[4..8].copy_from_slice(&str_off.to_le_bytes());
                str_off += r.target_name.len() as u32 + 1;
            }
            entry[8..12].copy_from_slice(&0u32.to_le_bytes());
            entry[12..14].copy_from_slice(&0i16.to_le_bytes()); // UNDEFINED
            entry[14..16].copy_from_slice(&0u16.to_le_bytes());
            entry[16] = 2; // EXTERNAL
            buf.extend_from_slice(&entry);
            seen.insert(r.target_name.clone());
        }
    }
    Ok(buf)
}

fn build_relocs(relocs: &[Reloc], symtab: &[u8], strtab: &str) -> Vec<u8> {
    let mut buf = Vec::new();
    for r in relocs {
        // VirtualAddress
        buf.extend_from_slice(&(r.offset as u32).to_le_bytes());

        // SymbolTableIndex
        let sym_idx = find_symbol_index(symtab, strtab, &r.target_name);
        buf.extend_from_slice(&sym_idx.to_le_bytes());

        // Type: IMAGE_REL_AMD64_REL32 = 0x0004
        buf.extend_from_slice(&0x0004u16.to_le_bytes());
    }
    buf
}

fn find_symbol_index(symtab: &[u8], strtab: &str, name: &str) -> u32 {
    for i in 0..(symtab.len() / 18) {
        let off = i * 18;
        let sym_name = if symtab[off..off+4] == [0, 0, 0, 0] {
            // Long name: bytes 4-7 contain offset into strtab (including 4-byte length prefix)
            let str_off = u32::from_le_bytes(symtab[off+4..off+8].try_into().unwrap_or([0; 4])) as usize;
            // str_off includes the 4-byte length prefix that precedes the string data in the file,
            // but `strtab` contains only the string data (without prefix), so subtract 4
            let actual_off = str_off.saturating_sub(4);
            let end = strtab[actual_off..].find('\0').map(|e| actual_off + e).unwrap_or(strtab.len());
            Some(strtab[actual_off..end].to_string())
        } else {
            let mut end = off + 8;
            while end > off && symtab[end-1] == 0 { end -= 1; }
            Some(String::from_utf8_lossy(&symtab[off..end]).to_string())
        };

        if let Some(s) = sym_name {
            if s == name { return i as u32; }
        }
    }
    0
}
