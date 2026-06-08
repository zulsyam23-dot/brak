// ── Shared ELF relocatable object parser ─────────────────
// Used by elf.rs, pe.rs, and macho.rs linkers

use std::collections::HashMap;
use brak_core::Result;

// ELF constants
pub const EI_CLASS: usize = 4;
pub const EI_PAD: usize = 8;
pub const ELFCLASS64: u8 = 2;
pub const ELFDATA2LSB: u8 = 1;
pub const EV_CURRENT: u8 = 1;
pub const ET_REL: u16 = 1;
pub const EM_X86_64: u16 = 62;
pub const SHT_PROGBITS: u32 = 1;
pub const SHT_SYMTAB: u32 = 2;
pub const SHT_STRTAB: u32 = 3;
pub const SHT_RELA: u32 = 4;
pub const SHF_EXECINSTR: u64 = 4;
pub const STB_GLOBAL: u8 = 1;

// Relocation types
pub const R_X86_64_64: u32 = 1;
pub const R_X86_64_PC32: u32 = 2;
pub const R_X86_64_PLT32: u32 = 4;
pub const R_X86_64_32S: u32 = 11;

#[derive(Clone)]
pub struct ParsedSymbol {
    pub name: String,
    pub st_value: u64,
    pub st_info: u8,
    pub st_shndx: u16,
}

#[derive(Clone, Debug)]
pub struct RelaEntry {
    pub r_offset: u64,
    pub r_type: u32,
    pub r_sym: u32,
    pub r_addend: i64,
}

pub struct ParsedElf {
    pub symbols: Vec<ParsedSymbol>,
    pub text_data: Vec<u8>,
    pub rela_text: Vec<RelaEntry>,
    pub debug_data: Vec<u8>,
    pub debug_sections: Vec<(String, Vec<u8>)>,
}

pub fn parse_elf(data: &[u8]) -> Result<ParsedElf> {
    if data.len() < 64 || data[0..4] != [0x7f, b'E', b'L', b'F'] {
        return Err("not a valid ELF file".into());
    }
    if read_u16(data, 16) != ET_REL {
        return Err(format!("not a relocatable ELF object (e_type={})", read_u16(data, 16)).into());
    }

    let shoff = read_u64(data, 40) as usize;
    let shentsize = read_u16(data, 58) as usize;
    let shnum = read_u16(data, 60) as usize;
    let shstrndx = read_u16(data, 62) as usize;

    if shentsize != 64 {
        return Err(format!("unexpected section header entry size: {shentsize}").into());
    }

    // Read section headers + shstrtab
    let mut sections = Vec::new();
    let mut shstrtab_data = Vec::new();

    for i in 0..shnum {
        let sh_off = shoff + i * shentsize;
        let sh_name = read_u32(data, sh_off) as usize;
        let sh_type = read_u32(data, sh_off + 4);
        let sh_flags = read_u64(data, sh_off + 8);
        let sh_offset = read_u64(data, sh_off + 24);
        let sh_size = read_u64(data, sh_off + 32);
        let sh_link = read_u32(data, sh_off + 40);
        let sh_info = read_u32(data, sh_off + 44);

        if i == shstrndx {
            let start = sh_offset as usize;
            let end = start + sh_size as usize;
            if end <= data.len() {
                shstrtab_data = data[start..end].to_vec();
            }
        }

        let name = if !shstrtab_data.is_empty() {
            read_str(&shstrtab_data, sh_name)
        } else {
            String::new()
        };

        sections.push(Shdr {
            name,
            sh_type,
            sh_flags,
            sh_offset,
            sh_size,
            sh_link,
            sh_info,
        });
    }

    // Retry reading shstrtab if not found in first pass
    if shstrtab_data.is_empty() && shstrndx < sections.len() {
        let s = &sections[shstrndx];
        let start = s.sh_offset as usize;
        let end = start + s.sh_size as usize;
        if end <= data.len() {
            shstrtab_data = data[start..end].to_vec();
        }
        for i in 0..shnum {
            let sh_off = shoff + i * shentsize;
            let sh_name = read_u32(data, sh_off) as usize;
            sections[i].name = read_str(&shstrtab_data, sh_name);
        }
    }

    // Extract .text, .symtab, .strtab, .rela.text, .debug_*
    let mut text_data = Vec::new();
    let mut symtab_data: &[u8] = &[];
    let mut strtab_data: &[u8] = &[];
    let mut rela_entries = Vec::new();
    let mut debug_sections: Vec<(String, Vec<u8>)> = Vec::new();

    for sec in &sections {
        let start = sec.sh_offset as usize;
        let end = start + sec.sh_size as usize;
        match sec.sh_type {
            SHT_PROGBITS if sec.sh_flags & SHF_EXECINSTR != 0 => {
                if end <= data.len() {
                    text_data = data[start..end].to_vec();
                }
            }
            SHT_SYMTAB => {
                if end <= data.len() {
                    symtab_data = &data[start..end];
                }
            }
            SHT_STRTAB if sec.name == ".strtab" => {
                if end <= data.len() {
                    strtab_data = &data[start..end];
                }
            }
            SHT_RELA => {
                if end <= data.len() {
                    let count = sec.sh_size as usize / 24;
                    for j in 0..count {
                        let r_off = start + j * 24;
                        let r_offset = read_u64(data, r_off);
                        let r_info = read_u64(data, r_off + 8);
                        let r_addend = read_i64(data, r_off + 16);
                        let r_type = (r_info & 0xffffffff) as u32;
                        let r_sym = (r_info >> 32) as u32;
                        rela_entries.push(RelaEntry { r_offset, r_type, r_sym, r_addend });
                    }
                }
            }
            _ if sec.name.starts_with(".debug_") => {
                if end <= data.len() {
                    let section_data = data[start..end].to_vec();
                    debug_sections.push((sec.name.clone(), section_data));
                }
            }
            _ => {}
        }
    }

    // Parse symbol table
    let mut symbols = Vec::new();
    if !symtab_data.is_empty() {
        let count = symtab_data.len() / 24;
        for j in 0..count {
            let s_off = j * 24;
            let st_name = read_u32(symtab_data, s_off);
            let st_info = read_u8(symtab_data, s_off + 4);
            let st_shndx = read_u16(symtab_data, s_off + 6);
            let st_value = read_u64(symtab_data, s_off + 8);
            let name = read_str(strtab_data, st_name as usize);
            symbols.push(ParsedSymbol { name, st_value, st_info, st_shndx });
        }
    }

    Ok(ParsedElf { symbols, text_data, rela_text: rela_entries, debug_data: Vec::new(), debug_sections })
}

pub fn parse_macho(data: &[u8]) -> Result<ParsedElf> {
    if data.len() < 32 {
        return Err("not a valid Mach-O file".into());
    }
    let magic = read_u32(data, 0);
    if magic != 0xFEEDFACF {
        return Err(format!("unsupported Mach-O magic: 0x{:x}", magic).into());
    }

    let ncmds = read_u32(data, 16) as usize;
    let mut offset = 32;
    let mut text_data = Vec::new();
    let mut debug_sections: Vec<(String, Vec<u8>)> = Vec::new();
    let mut symoff = 0;
    let mut nsyms = 0;
    let mut stroff = 0;
    let mut reloc_off = 0;
    let mut nreloc = 0;

    for _ in 0..ncmds {
        let cmd = read_u32(data, offset);
        let cmdsize = read_u32(data, offset + 4) as usize;

        match cmd {
            0x19 => { // LC_SEGMENT_64
                let nsects = read_u32(data, offset + 64) as usize;
                for j in 0..nsects {
                    let sect_off = offset + 72 + (j * 80);
                    let sectname = read_str(data, sect_off);
                    let sect_size = read_u64(data, sect_off + 40) as usize;
                    let sect_file_off = read_u32(data, sect_off + 48) as usize;
                    if sectname == "__text" {
                        text_data = data[sect_file_off..sect_file_off + sect_size].to_vec();
                        reloc_off = read_u32(data, sect_off + 56) as usize;
                        nreloc = read_u32(data, sect_off + 60) as usize;
                    } else if sectname.starts_with("__debug_") {
                        if sect_file_off + sect_size <= data.len() {
                            let section_data = data[sect_file_off..sect_file_off + sect_size].to_vec();
                            debug_sections.push((sectname.clone(), section_data));
                        }
                    }
                }
            }
            0x2 => { // LC_SYMTAB
                symoff = read_u32(data, offset + 8) as usize;
                nsyms = read_u32(data, offset + 12) as usize;
                stroff = read_u32(data, offset + 16) as usize;
            }
            _ => {}
        }
        offset += cmdsize;
    }

    let mut symbols = Vec::new();
    for i in 0..nsyms {
        let n_off = symoff + (i * 16);
        let n_strx = read_u32(data, n_off) as usize;
        let n_type = data[n_off + 4];
        let n_sect = data[n_off + 5];
        let n_value = read_u64(data, n_off + 8);
        
        let name = read_str(data, stroff + n_strx);
        symbols.push(ParsedSymbol {
            name,
            st_value: n_value,
            st_info: n_type,
            st_shndx: n_sect as u16,
        });
    }

    // Read relocations
    let mut rela_text = Vec::new();
    for i in 0..nreloc {
        let r_off = reloc_off + (i * 8);
        if r_off + 8 > data.len() {
            break;
        }
        let r_address = read_u32(data, r_off) as u64; // instruction start
        let second = read_u32(data, r_off + 4);
        
        let r_symbolnum = second & 0x00FFFFFF;
        let r_pcrel = (second >> 24) & 1;
        let _r_length = (second >> 25) & 3;
        let r_extern = (second >> 27) & 1;
        let r_type = (second >> 28) & 0xF;

        // Only handle external relocations for now
        if r_extern == 1 && r_pcrel == 1 {
            // Convert r_address from instruction start to offset of 4-byte value
            // X86_64_RELOC_BRANCH: instruction is 5 bytes (call rel32)
            let r_offset = r_address + 1;
            
            let elf_type = match r_type {
                2 => R_X86_64_PLT32, // X86_64_RELOC_BRANCH
                _ => r_type, // pass through
            };

            rela_text.push(RelaEntry {
                r_offset,
                r_type: elf_type,
                r_sym: r_symbolnum,
                r_addend: 0,
            });
        }
    }

    Ok(ParsedElf {
        symbols,
        text_data,
        rela_text,
        debug_data: Vec::new(),
        debug_sections,
    })
}

pub fn parse_coff(data: &[u8]) -> Result<ParsedElf> {
    if data.len() < 20 {
        return Err("not a valid COFF file".into());
    }
    let machine = read_u16(data, 0);
    if machine != 0x8664 {
        return Err(format!("unsupported COFF machine: 0x{:x}", machine).into());
    }

    let num_sections = read_u16(data, 2);
    let symtab_off = read_u32(data, 8) as usize;
    let num_symbols = read_u32(data, 12) as usize;

    // Find .text and .debug$S sections
    let mut text_data = Vec::new();
    let mut debug_data = Vec::new();
    let mut _text_sec_idx = 0;
    for i in 0..num_sections {
        let sec_off = 20 + (i as usize * 40);
        if sec_off + 40 > data.len() { break; }
        let name = read_str(data, sec_off);
        let raw_data_off = read_u32(data, sec_off + 20) as usize;
        let raw_data_size = read_u32(data, sec_off + 16) as usize;
        if name == ".text" {
            if raw_data_off + raw_data_size <= data.len() {
                text_data = data[raw_data_off..raw_data_off + raw_data_size].to_vec();
            }
            _text_sec_idx = i + 1;
        } else if name == ".debug$S" {
            if raw_data_off + raw_data_size <= data.len() {
                debug_data = data[raw_data_off..raw_data_off + raw_data_size].to_vec();
            }
        }
    }

    // Read symbols
    let mut symbols = Vec::new();
    let strtab_off = symtab_off + (num_symbols * 18);
    let mut i = 0;
    while i < num_symbols {
        let sym_off = symtab_off + (i * 18);
        let name = if data[sym_off..sym_off + 4] == [0, 0, 0, 0] {
            let off = read_u32(data, sym_off + 4) as usize;
            read_str(data, strtab_off + off)
        } else {
            let mut end = sym_off + 8;
            while end > sym_off && data[end - 1] == 0 {
                end -= 1;
            }
            String::from_utf8_lossy(&data[sym_off..end]).to_string()
        };

        let value = read_u32(data, sym_off + 8) as u64;
        let sec_num = read_i16(data, sym_off + 12) as u16;
        let aux_symbols = data[sym_off + 17] as usize;

        symbols.push(ParsedSymbol {
            name,
            st_value: value,
            st_info: STB_GLOBAL << 4, // Shift to match ELF-like st_info
            st_shndx: sec_num,
        });

        i += 1 + aux_symbols;
    }

    // Read relocations
    let mut rela_text = Vec::new();
    for i in 0..num_sections {
        let sec_off = 20 + (i as usize * 40);
        let name = read_str(data, sec_off);
        if name == ".text" {
            let num_relocs = read_u16(data, sec_off + 32) as usize;
            let reloc_off = read_u32(data, sec_off + 24) as usize;
            for j in 0..num_relocs {
                let r_off = reloc_off + (j * 10);
                let v_addr = read_u32(data, r_off) as u64;
                let sym_idx = read_u32(data, r_off + 4);
                let r_type = read_u16(data, r_off + 8) as u32;
                
                // COFF IMAGE_REL_AMD64_REL32 = 0x0004
                // Map to R_X86_64_PC32 (2) for shared logic
                let mapped_type = if r_type == 4 { R_X86_64_PC32 } else { r_type };
                
                rela_text.push(RelaEntry {
                    r_offset: v_addr,
                    r_type: mapped_type,
                    r_sym: sym_idx,
                    r_addend: 0, // COFF relocs usually don't use addends like ELF
                });
            }
        }
    }

    Ok(ParsedElf {
        symbols,
        text_data,
        rela_text,
        debug_data,
        debug_sections: Vec::new(),
    })
}

struct Shdr {
    #[allow(dead_code)]
    name: String,
    sh_type: u32,
    sh_flags: u64,
    sh_offset: u64,
    sh_size: u64,
    #[allow(dead_code)]
    sh_link: u32,
    #[allow(dead_code)]
    sh_info: u32,
}

// ── Symbol resolution ──────────────────────────────────

pub fn build_global_sym_map(parsed: &[ParsedElf]) -> HashMap<String, (usize, usize)> {
    let mut map = HashMap::new();
    for (oi, p) in parsed.iter().enumerate() {
        for (si, sym) in p.symbols.iter().enumerate() {
            if !sym.name.is_empty() && (sym.st_info >> 4) == STB_GLOBAL {
                map.insert(sym.name.clone(), (oi, si));
            }
        }
    }
    map
}

pub fn resolve_sym_addr(
    parsed: &[ParsedElf],
    global_syms: &HashMap<String, (usize, usize)>,
    sym: &ParsedSymbol,
    text_bases: &[u64],
    base_addr: u64,
    offset_base: u64,
    obj_oi: usize,
) -> Result<u64> {
    if sym.st_shndx == 0 {
        match global_syms.get(&sym.name) {
            Some(&(def_oi, def_si)) => {
                let def_sym = &parsed[def_oi].symbols[def_si];
                let def_base = text_bases[def_oi];
                Ok(base_addr + offset_base + def_base + def_sym.st_value)
            }
            None => Err(format!("undefined symbol: {}", sym.name).into()),
        }
    } else {
        Ok(base_addr + offset_base + text_bases[obj_oi] + sym.st_value)
    }
}

pub fn find_entry_offset(parsed: &[ParsedElf], global_syms: &HashMap<String, (usize, usize)>, text_bases: &[u64], entry: &str) -> u64 {
    match global_syms.get(entry) {
        Some(&(oi, si)) => {
            let sym = &parsed[oi].symbols[si];
            text_bases[oi] + sym.st_value
        }
        None => 0,
    }
}

pub fn apply_reloc(
    text: &mut [u8],
    patch_offset: usize,
    r_type: u32,
    sym_addr: u64,
    base_addr: u64,
    target_offset: u64,
) -> Result<()> {
    let patch_addr = base_addr + target_offset;
    match r_type {
        R_X86_64_64 => {
            let val = sym_addr.wrapping_add(0); // A = 0
            if patch_offset + 8 <= text.len() {
                text[patch_offset..patch_offset + 8].copy_from_slice(&val.to_le_bytes());
            }
        }
        R_X86_64_PC32 | R_X86_64_PLT32 => {
            let val = (sym_addr as i64).wrapping_sub((patch_addr + 4) as i64); // PC-relative to end of 4-byte displacement
            if patch_offset + 4 <= text.len() {
                text[patch_offset..patch_offset + 4].copy_from_slice(&(val as i32).to_le_bytes());
            }
        }
        R_X86_64_32S => {
            let val = sym_addr as i32;
            if patch_offset + 4 <= text.len() {
                text[patch_offset..patch_offset + 4].copy_from_slice(&val.to_le_bytes());
            }
        }
        _ => return Err(format!("unsupported relocation type: {r_type}").into()),
    }
    Ok(())
}

// ── Binary reader helpers ─────────────────────────────

fn read_u16(buf: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(buf[offset..offset + 2].try_into().unwrap())
}

fn read_u32(buf: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(buf[offset..offset + 4].try_into().unwrap())
}

fn read_u64(buf: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(buf[offset..offset + 8].try_into().unwrap())
}

fn read_i64(buf: &[u8], offset: usize) -> i64 {
    i64::from_le_bytes(buf[offset..offset + 8].try_into().unwrap())
}

fn read_i16(buf: &[u8], offset: usize) -> i16 {
    i16::from_le_bytes(buf[offset..offset + 2].try_into().unwrap())
}

fn read_u8(buf: &[u8], offset: usize) -> u8 {
    buf[offset]
}

fn read_str(tab: &[u8], offset: usize) -> String {
    if offset >= tab.len() {
        return String::new();
    }
    let end = tab[offset..].iter().position(|&b| b == 0).unwrap_or(tab.len() - offset);
    String::from_utf8_lossy(&tab[offset..offset + end]).to_string()
}

// ── Binary writer helpers ─────────────────────────────

pub fn write_u16(buf: &mut Vec<u8>, val: u16) { buf.extend_from_slice(&val.to_le_bytes()); }
pub fn write_u32(buf: &mut Vec<u8>, val: u32) { buf.extend_from_slice(&val.to_le_bytes()); }
pub fn write_u64(buf: &mut Vec<u8>, val: u64) { buf.extend_from_slice(&val.to_le_bytes()); }
pub fn write_u8(buf: &mut Vec<u8>, val: u8) { buf.push(val); }

// ── Shared linker helpers ──────────────────────────────

pub fn merge_text(parsed: &[ParsedElf]) -> (Vec<u8>, Vec<u64>) {
    let mut merged = Vec::new();
    let mut bases = Vec::new();
    for p in parsed {
        bases.push(merged.len() as u64);
        merged.extend_from_slice(&p.text_data);
    }
    (merged, bases)
}

pub fn apply_all_relocs(
    parsed: &[ParsedElf],
    global_syms: &std::collections::HashMap<String, (usize, usize)>,
    merged_text: &[u8],
    text_bases: &[u64],
    base_addr: u64,
    target_offset_base: u64,
) -> Result<Vec<u8>> {
    let mut text = merged_text.to_vec();
    for (oi, p) in parsed.iter().enumerate() {
        let obj_text_base = text_bases[oi];
        for rela in &p.rela_text {
            let sym = &p.symbols[rela.r_sym as usize];
            let sym_addr = resolve_sym_addr(parsed, global_syms, sym, text_bases, base_addr, target_offset_base, oi)?;
            let patch_offset = (obj_text_base + rela.r_offset) as usize;
            let target_offset = target_offset_base + obj_text_base + rela.r_offset;
            apply_reloc(&mut text, patch_offset, rela.r_type, sym_addr, base_addr, target_offset)?;
        }
    }
    Ok(text)
}
