use crate::x86_64::*;
use brak_ir_lir::lir::LirProgram;

pub fn write_macho(program: &LirProgram) -> Result<Vec<u8>, CodegenError> {
    let mut buf = Vec::new();

    let (text_data, relocs, line_entries) = emit_text(program)?;
    let dwarf = crate::dwarf::build_dwarf(program, &text_data, &line_entries);
    let strtab = build_strtab(program, &relocs);
    let symtab = build_symtab(program, &strtab, &relocs)?;
    let reloc_data = build_relocations(&relocs, &symtab, &strtab);
    let reloc_size = reloc_data.len() as u32;

    // Data offsets
    let text_fileoff = 32 + 152 + 24 + 392; // header + 2 segcmds + symtab
    let dwarf_fileoff = text_fileoff + text_data.len() as u32;
    let reloff = dwarf_fileoff;
    let symoff = reloff + reloc_size;
    let stroff = symoff + symtab.len() as u32;

    // ── mach_header_64 (32 bytes) ──────────────────────
    buf.extend_from_slice(&0xFEEDFACFu32.to_le_bytes()); // MH_MAGIC_64
    buf.extend_from_slice(&0x01000007u32.to_le_bytes()); // CPU_TYPE_X86_64
    buf.extend_from_slice(&3u32.to_le_bytes());          // CPU_SUBTYPE_X86_64_ALL
    buf.extend_from_slice(&1u32.to_le_bytes());          // MH_OBJECT
    buf.extend_from_slice(&3u32.to_le_bytes());          // ncmds (2 seg + LC_SYMTAB)
    let sizeofcmds: u32 = 152 + 392 + 24;
    buf.extend_from_slice(&sizeofcmds.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());          // flags
    buf.extend_from_slice(&0u32.to_le_bytes());          // reserved

    // ── LC_SEGMENT_64 — __TEXT (152 bytes) ─────────────
    buf.extend_from_slice(&0x19u32.to_le_bytes());       // LC_SEGMENT_64
    buf.extend_from_slice(&152u32.to_le_bytes());        // cmdsize (72 + 1*80)
    buf.extend_from_slice(b"__TEXT\0\0\0\0\0\0\0\0\0\0\0"); // segname
    buf.extend_from_slice(&0u64.to_le_bytes());          // vmaddr
    buf.extend_from_slice(&(text_data.len() as u64).to_le_bytes()); // vmsize
    buf.extend_from_slice(&(text_fileoff as u64).to_le_bytes()); // fileoff
    buf.extend_from_slice(&(text_data.len() as u64).to_le_bytes()); // filesize
    buf.extend_from_slice(&7u32.to_le_bytes());          // maxprot (RWX)
    buf.extend_from_slice(&7u32.to_le_bytes());          // initprot (RWX)
    buf.extend_from_slice(&1u32.to_le_bytes());          // nsects
    buf.extend_from_slice(&0u32.to_le_bytes());          // flags

    // Section __TEXT,__text (80 bytes)
    buf.extend_from_slice(b"__text\0\0\0\0\0\0\0\0\0\0");
    buf.extend_from_slice(b"__TEXT\0\0\0\0\0\0\0\0\0\0");
    buf.extend_from_slice(&0u64.to_le_bytes());          // addr
    buf.extend_from_slice(&(text_data.len() as u64).to_le_bytes()); // size
    buf.extend_from_slice(&(text_fileoff as u32).to_le_bytes()); // offset
    buf.extend_from_slice(&0u32.to_le_bytes());          // align
    buf.extend_from_slice(&reloff.to_le_bytes());        // reloff
    buf.extend_from_slice(&(relocs.len() as u32).to_le_bytes()); // nreloc
    buf.extend_from_slice(&0x80000400u32.to_le_bytes()); // flags
    buf.extend_from_slice(&0u32.to_le_bytes());          // reserved1
    buf.extend_from_slice(&0u32.to_le_bytes());          // reserved2
    buf.extend_from_slice(&0u32.to_le_bytes());          // reserved3

    // ── LC_SEGMENT_64 — __DWARF (392 bytes) ────────────
    let dwarf_data = [
        (&dwarf.debug_line[..], "__debug_line"),
        (&dwarf.debug_info[..], "__debug_info"),
        (&dwarf.debug_abbrev[..], "__debug_abbrev"),
        (&dwarf.debug_str[..], "__debug_str"),
    ];
    let dwarf_section_count: u32 = dwarf_data.len() as u32;
    let dwarf_seg_cmd_size: u32 = 72 + dwarf_section_count * 80;
    let dwarf_seg_data_off: u32 = dwarf_fileoff;

    buf.extend_from_slice(&0x19u32.to_le_bytes());       // LC_SEGMENT_64
    buf.extend_from_slice(&dwarf_seg_cmd_size.to_le_bytes());
    buf.extend_from_slice(b"__DWARF\0\0\0\0\0\0\0\0\0\0"); // segname
    buf.extend_from_slice(&0u64.to_le_bytes());          // vmaddr
    buf.extend_from_slice(&0u64.to_le_bytes());          // vmsize
    buf.extend_from_slice(&(dwarf_seg_data_off as u64).to_le_bytes()); // fileoff
    let dwarf_total_size: u64 = dwarf_data.iter().map(|(d, _)| d.len() as u64).sum();
    buf.extend_from_slice(&dwarf_total_size.to_le_bytes()); // filesize
    buf.extend_from_slice(&0u32.to_le_bytes());          // maxprot
    buf.extend_from_slice(&0u32.to_le_bytes());          // initprot
    buf.extend_from_slice(&dwarf_section_count.to_le_bytes()); // nsects
    buf.extend_from_slice(&0u32.to_le_bytes());          // flags

    let mut dwarf_sect_off = dwarf_seg_data_off;
    for &(data, name) in &dwarf_data {
        let name_bytes = name.as_bytes();
        let mut sect_name = [0u8; 16];
        sect_name[..name_bytes.len()].copy_from_slice(name_bytes);
        buf.extend_from_slice(&sect_name);
        buf.extend_from_slice(b"__DWARF\0\0\0\0\0\0\0\0\0"); // segname (padded)
        buf.extend_from_slice(&0u64.to_le_bytes());          // addr
        buf.extend_from_slice(&(data.len() as u64).to_le_bytes()); // size
        buf.extend_from_slice(&dwarf_sect_off.to_le_bytes()); // offset
        buf.extend_from_slice(&0u32.to_le_bytes());          // align
        buf.extend_from_slice(&0u32.to_le_bytes());          // reloff
        buf.extend_from_slice(&0u32.to_le_bytes());          // nreloc
        buf.extend_from_slice(&0u32.to_le_bytes());          // flags
        buf.extend_from_slice(&0u32.to_le_bytes());          // reserved1
        buf.extend_from_slice(&0u32.to_le_bytes());          // reserved2
        buf.extend_from_slice(&0u32.to_le_bytes());          // reserved3
        dwarf_sect_off += data.len() as u32;
    }

    // ── LC_SYMTAB (24 bytes) ───────────────────────────
    buf.extend_from_slice(&0x2u32.to_le_bytes());        // LC_SYMTAB
    buf.extend_from_slice(&24u32.to_le_bytes());         // cmdsize
    buf.extend_from_slice(&symoff.to_le_bytes());
    buf.extend_from_slice(&(symtab.len() as u32 / 16).to_le_bytes()); // nsyms
    buf.extend_from_slice(&stroff.to_le_bytes());
    buf.extend_from_slice(&(strtab.len() as u32).to_le_bytes());

    // ── Data ───────────────────────────────────────────
    buf.extend_from_slice(&text_data);  // __TEXT,__text
    // DWARF data
    for &(data, _) in &dwarf_data {
        buf.extend_from_slice(data);
    }
    // Relocations
    buf.extend_from_slice(&reloc_data);
    // Symbol table
    buf.extend_from_slice(&symtab);
    // String table
    buf.extend_from_slice(strtab.as_bytes());

    Ok(buf)
}

fn build_strtab(program: &LirProgram, relocs: &[Reloc]) -> String {
    let mut s = String::new();
    s.push(' '); // First char unused
    for func in &program.functions {
        s.push_str(&func.name);
        s.push('\0');
    }
    let mut seen: std::collections::HashSet<String> = program.functions.iter().map(|f| f.name.clone()).collect();
    for r in relocs {
        if !seen.contains(&r.target_name) {
            s.push_str(&r.target_name);
            s.push('\0');
            seen.insert(r.target_name.clone());
        }
    }
    s
}

fn build_symtab(program: &LirProgram, strtab: &str, relocs: &[Reloc]) -> Result<Vec<u8>, CodegenError> {
    let mut buf = Vec::new();
    let mut text_off = 0u64;

    let mut func_labels = std::collections::HashMap::new();
    let mut block_labels = std::collections::HashMap::new();

    // Internal functions
    for func in &program.functions {
        let name_off = strtab.find(&func.name).unwrap_or(0) as u32;
        buf.extend_from_slice(&name_off.to_le_bytes());
        buf.push(0x0f); // n_type = N_EXT | N_SECT
        buf.push(1);    // n_sect = 1 (__text)
        buf.extend_from_slice(&0u16.to_le_bytes()); // n_desc
        buf.extend_from_slice(&text_off.to_le_bytes()); // n_value

        let mut dummy_lines = Vec::new();
        let (code, _) = emit_function(func, &mut func_labels, &mut block_labels, &mut dummy_lines)?;
        text_off += code.len() as u64;
    }

    // External symbols from relocs (undefined)
    let mut seen: std::collections::HashSet<String> = program.functions.iter().map(|f| f.name.clone()).collect();
    for r in relocs {
        if !seen.contains(&r.target_name) {
            let name_off = strtab.find(&r.target_name).unwrap_or(0) as u32;
            buf.extend_from_slice(&name_off.to_le_bytes());
            buf.push(0x01); // n_type = N_EXT (undefined)
            buf.push(0);    // n_sect = NO_SECT
            buf.extend_from_slice(&0u16.to_le_bytes()); // n_desc
            buf.extend_from_slice(&0u64.to_le_bytes()); // n_value = 0
            seen.insert(r.target_name.clone());
        }
    }

    Ok(buf)
}

fn build_relocations(relocs: &[Reloc], symtab: &[u8], strtab: &str) -> Vec<u8> {
    let mut buf = Vec::new();
    for r in relocs {
        // Find symbol index in symtab (16 bytes per entry)
        let sym_idx = find_macho_symbol_index(symtab, strtab, &r.target_name);

        // r_address: instruction start = offset - 1 (offset points to displacement field)
        let r_address = (r.offset as u32).wrapping_sub(1);

        // Packed second dword: r_symbolnum:24 | r_pcrel:1 | r_length:2 | r_extern:1 | r_type:4
        // X86_64_RELOC_BRANCH = 2
        let r_symbolnum = sym_idx & 0x00FFFFFF;
        let r_pcrel: u32 = 1;    // PC-relative
        let r_length: u32 = 2;   // 4 bytes (32-bit)
        let r_extern: u32 = 1;   // external symbol
        let r_type: u32 = 2;     // X86_64_RELOC_BRANCH

        let second = r_symbolnum
            | (r_pcrel << 24)
            | (r_length << 25)
            | (r_extern << 27)
            | (r_type << 28);

        buf.extend_from_slice(&r_address.to_le_bytes());
        buf.extend_from_slice(&second.to_le_bytes());
    }
    buf
}

fn find_macho_symbol_index(symtab: &[u8], strtab: &str, name: &str) -> u32 {
    // Symtab entries are 16 bytes: n_strx(4) | n_type(1) | n_sect(1) | n_desc(2) | n_value(8)
    for i in 0..(symtab.len() / 16) {
        let off = i * 16;
        let str_off = u32::from_le_bytes(symtab[off..off+4].try_into().unwrap_or([0; 4])) as usize;
        if str_off > 0 && str_off < strtab.len() {
            let end = strtab[str_off..].find('\0').map(|e| str_off + e).unwrap_or(strtab.len());
            let sym_name = &strtab[str_off..end];
            if sym_name == name {
                return i as u32;
            }
        }
    }
    0
}

