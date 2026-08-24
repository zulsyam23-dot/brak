use std::collections::HashMap;
use brak_ir_lir::lir::LirProgram;
use crate::x86_64::*;

const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
const EHDR_SIZE: u64 = 64;
const PHDR_SIZE: u64 = 56;
const SHDR_SIZE: u64 = 64;
const ET_REL: u16 = 1;
const ET_EXEC: u16 = 2;
const EM_X86_64: u16 = 62;
const PT_LOAD: u32 = 1;
const PF_R: u32 = 4;
const PF_X: u32 = 1;
const SHT_PROGBITS: u32 = 1;
const SHT_SYMTAB: u32 = 2;
const SHT_STRTAB: u32 = 3;
const SHT_RELA: u32 = 4;
const SHF_ALLOC: u64 = 2;
const SHF_EXECINSTR: u64 = 4;
const STB_GLOBAL: u8 = 1;
const STT_FUNC: u8 = 2;

pub fn write_elf(program: &LirProgram) -> Result<Vec<u8>, CodegenError> {
    let mut buf = Vec::new();

    let (text_data, relocs, line_entries) = emit_text(program)?;
    let dwarf = crate::dwarf::build_dwarf(program, &text_data, &line_entries);

    let shstrtab_content = b"\x00.shstrtab\x00.text\x00.symtab\x00.strtab\x00.rela.text\x00.debug_line\x00.debug_info\x00.debug_abbrev\x00.debug_str\x00";
    let strtab_content = build_strtab(program, &relocs);
    let symtab_entries = build_symtab(program, &strtab_content, &relocs)?;
    let rela_entries = build_rela_entries(&relocs, &symtab_entries, &strtab_content);

    let text_offset = EHDR_SIZE;
    let text_size = text_data.len() as u64;

    let after_text = text_offset + text_size;
    let text_padding = (8 - (text_size % 8)) % 8;

    let shstrtab_offset = after_text + text_padding;
    let shstrtab_size = shstrtab_content.len() as u64;

    let symtab_offset = shstrtab_offset + shstrtab_size;
    let symtab_size = symtab_entries.len() as u64 * 24;

    let strtab_offset = symtab_offset + symtab_size;
    let strtab_size = strtab_content.len() as u64 + 1;

    let rel_text_offset = strtab_offset + strtab_size;
    let rel_text_size = rela_entries.len() as u64 * 24;

    let debug_line_offset = rel_text_offset + rel_text_size;
    let debug_line_size = dwarf.debug_line.len() as u64;

    let debug_info_offset = debug_line_offset + debug_line_size;
    let debug_info_size = dwarf.debug_info.len() as u64;

    let debug_abbrev_offset = debug_info_offset + debug_info_size;
    let debug_abbrev_size = dwarf.debug_abbrev.len() as u64;

    let debug_str_offset = debug_abbrev_offset + debug_abbrev_size;
    let debug_str_size = dwarf.debug_str.len() as u64;

    let section_count: u16 = 10;
    let shstrtab_idx: u16 = 1;

    let shoff_raw = debug_str_offset + debug_str_size;
    let shoff_pad = (8 - (shoff_raw % 8)) % 8;
    let shoff = shoff_raw + shoff_pad;

    // ELF header
    buf.extend_from_slice(&ELF_MAGIC);
    buf.push(2);
    buf.push(1);
    buf.push(1);
    buf.push(0);
    buf.extend_from_slice(&[0u8; 8]);
    buf.extend_from_slice(&ET_REL.to_le_bytes());
    buf.extend_from_slice(&EM_X86_64.to_le_bytes());
    buf.extend_from_slice(&1u32.to_le_bytes());
    buf.extend_from_slice(&0u64.to_le_bytes());
    buf.extend_from_slice(&0u64.to_le_bytes());
    buf.extend_from_slice(&shoff.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&(EHDR_SIZE as u16).to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes());
    buf.extend_from_slice(&(SHDR_SIZE as u16).to_le_bytes());
    buf.extend_from_slice(&section_count.to_le_bytes());
    buf.extend_from_slice(&shstrtab_idx.to_le_bytes());

    // .text data
    buf.extend_from_slice(&text_data);
    buf.extend(std::iter::repeat_n(0, text_padding as usize));
    // .shstrtab
    buf.extend_from_slice(shstrtab_content);
    // .symtab
    for entry in &symtab_entries {
        buf.extend_from_slice(entry.as_slice());
    }
    // .strtab
    buf.extend_from_slice(strtab_content.as_bytes());
    buf.push(0);
    // .rela.text
    for entry in &rela_entries {
        buf.extend_from_slice(entry.as_slice());
    }
    // .debug_line
    buf.extend_from_slice(&dwarf.debug_line);
    // .debug_info
    buf.extend_from_slice(&dwarf.debug_info);
    // .debug_abbrev
    buf.extend_from_slice(&dwarf.debug_abbrev);
    // .debug_str
    buf.extend_from_slice(&dwarf.debug_str);

    // Section header table padding
    let pad = (8 - (buf.len() as u64 % 8)) % 8;
    buf.extend(std::iter::repeat_n(0, pad as usize));

    // Section headers
    // 0: null
    buf.extend(std::iter::repeat_n(0, SHDR_SIZE as usize));

    // 1: .shstrtab
    write_shdr(&mut buf, name_offset_u32(shstrtab_content, b".shstrtab"), SHT_STRTAB, 0, shstrtab_offset, shstrtab_size);
    // 2: .text
    write_shdr(&mut buf, name_offset_u32(shstrtab_content, b".text"), SHT_PROGBITS, SHF_ALLOC | SHF_EXECINSTR, text_offset, text_size);
    // 3: .symtab
    let mut sym_shdr = [0u8; SHDR_SIZE as usize];
    write_shdr_to_slice(&mut sym_shdr, name_offset_u32(shstrtab_content, b".symtab"), SHT_SYMTAB, 0, symtab_offset, symtab_size, 4, 1);
    buf.extend_from_slice(&sym_shdr);
    // 4: .strtab
    write_shdr(&mut buf, name_offset_u32(shstrtab_content, b".strtab"), SHT_STRTAB, 0, strtab_offset, strtab_size);
    // 5: .rela.text
    let mut rela_shdr = [0u8; SHDR_SIZE as usize];
    write_shdr_to_slice(&mut rela_shdr, name_offset_u32(shstrtab_content, b".rela.text"), SHT_RELA, 0, rel_text_offset, rel_text_size, 3, 2);
    buf.extend_from_slice(&rela_shdr);
    // 6: .debug_line
    write_shdr(&mut buf, name_offset_u32(shstrtab_content, b".debug_line"), SHT_PROGBITS, 0, debug_line_offset, debug_line_size);
    // 7: .debug_info
    write_shdr(&mut buf, name_offset_u32(shstrtab_content, b".debug_info"), SHT_PROGBITS, 0, debug_info_offset, debug_info_size);
    // 8: .debug_abbrev
    write_shdr(&mut buf, name_offset_u32(shstrtab_content, b".debug_abbrev"), SHT_PROGBITS, 0, debug_abbrev_offset, debug_abbrev_size);
    // 9: .debug_str
    write_shdr(&mut buf, name_offset_u32(shstrtab_content, b".debug_str"), SHT_PROGBITS, 0, debug_str_offset, debug_str_size);

    Ok(buf)
}

fn write_shdr_to_slice(
    buf: &mut [u8],
    name: u32,
    sh_type: u32,
    flags: u64,
    offset: u64,
    size: u64,
    link: u32,
    info: u32,
) {
    buf[0..4].copy_from_slice(&name.to_le_bytes());
    buf[4..8].copy_from_slice(&sh_type.to_le_bytes());
    buf[8..16].copy_from_slice(&flags.to_le_bytes());
    buf[24..32].copy_from_slice(&offset.to_le_bytes());
    buf[32..40].copy_from_slice(&size.to_le_bytes());
    buf[40..44].copy_from_slice(&link.to_le_bytes());
    buf[44..48].copy_from_slice(&info.to_le_bytes());
    buf[48..56].copy_from_slice(&8u64.to_le_bytes()); // sh_addralign
    if sh_type == SHT_SYMTAB {
        buf[56..64].copy_from_slice(&24u64.to_le_bytes()); // sh_entsize
    } else if sh_type == SHT_RELA {
        buf[56..64].copy_from_slice(&24u64.to_le_bytes()); // sh_entsize
    }
}


/// Build a 16-byte `_start` stub that calls the first function after itself and exits via syscall.
///
/// Layout (x86-64 Linux):
///   xor edi, edi           ; 31 ff       - rdi = 0
///   call <entry>           ; e8 xx xx xx xx
///   mov edi, eax           ; 89 c7       - rdi = return value
///   mov eax, 60            ; b8 3c 00 00 00  - SYS_exit
///   syscall                ; 0f 05
fn build_start_stub() -> [u8; 16] {
    // The entry function starts immediately after this stub at offset 16.
    // call instruction is at offset 2, length 5. rel32 = 16 - (2+5) = 9
    let mut stub = [0u8; 16];
    stub[0..2].copy_from_slice(&[0x31, 0xff]);                     // xor edi, edi
    stub[2..7].copy_from_slice(&[0xe8, 0x09, 0x00, 0x00, 0x00]); // call +9
    stub[7..9].copy_from_slice(&[0x89, 0xc7]);                    // mov edi, eax
    stub[9..14].copy_from_slice(&[0xb8, 0x3c, 0x00, 0x00, 0x00]); // mov eax, 60
    stub[14..16].copy_from_slice(&[0x0f, 0x05]);                  // syscall
    stub
}

pub fn write_elf_executable(program: &LirProgram, entry: &str) -> Result<Vec<u8>, CodegenError> {
    let base_addr: u64 = 0x400000;

    // Reorder functions so the entry function is first
    let mut functions = program.functions.clone();
    if let Some(pos) = functions.iter().position(|f| f.name == entry) {
        let func = functions.remove(pos);
        functions.insert(0, func);
    }
    let reordered = LirProgram {
        functions,
        extern_functions: program.extern_functions.clone(),
        structs: program.structs.clone(),
        enums: program.enums.clone(),
        string_table: program.string_table.clone(),
        files: program.files.clone(),
    };

    let stub = build_start_stub();
    let (user_text_data, _, _) = emit_text(&reordered)?;
    let full_text = [&stub[..], &user_text_data].concat();
    let text_size = full_text.len() as u64;

    let text_off = EHDR_SIZE + PHDR_SIZE;
    let entry_off = text_off;

    // Prepare sections for debuggability (simplified, no symtab/strtab for executable)
    let shstrtab_content = b"\x00.shstrtab\x00.text\x00";
    let shstrtab_off = text_off + text_size;
    let shstrtab_size = shstrtab_content.len() as u64;

    let shoff_raw = shstrtab_off + shstrtab_size;
    let shoff_pad = (8 - (shoff_raw % 8)) % 8;
    let shoff = shoff_raw + shoff_pad;

    let mut buf = Vec::new();

    // ELF header
    buf.extend_from_slice(&ELF_MAGIC);
    buf.push(2);   // 64-bit
    buf.push(1);   // little-endian
    buf.push(1);   // ELF version
    buf.push(0);   // OS/ABI
    buf.extend_from_slice(&[0u8; 8]); // padding
    buf.extend_from_slice(&ET_EXEC.to_le_bytes());
    buf.extend_from_slice(&EM_X86_64.to_le_bytes());
    buf.extend_from_slice(&1u32.to_le_bytes()); // version
    buf.extend_from_slice(&(base_addr + entry_off).to_le_bytes()); // e_entry
    buf.extend_from_slice(&64u64.to_le_bytes()); // e_phoff
    buf.extend_from_slice(&shoff.to_le_bytes()); // e_shoff
    buf.extend_from_slice(&0u32.to_le_bytes()); // e_flags
    buf.extend_from_slice(&(EHDR_SIZE as u16).to_le_bytes()); // e_ehsize
    buf.extend_from_slice(&(PHDR_SIZE as u16).to_le_bytes()); // e_phentsize
    buf.extend_from_slice(&1u16.to_le_bytes()); // e_phnum
    buf.extend_from_slice(&(SHDR_SIZE as u16).to_le_bytes());
    buf.extend_from_slice(&3u16.to_le_bytes()); // e_shnum (null + .shstrtab + .text)
    buf.extend_from_slice(&1u16.to_le_bytes()); // e_shstrndx (.shstrtab)

    // Program header: PT_LOAD
    buf.extend_from_slice(&PT_LOAD.to_le_bytes());   // p_type
    buf.extend_from_slice(&(PF_R | PF_X).to_le_bytes()); // p_flags
    buf.extend_from_slice(&text_off.to_le_bytes());  // p_offset
    buf.extend_from_slice(&base_addr.to_le_bytes()); // p_vaddr
    buf.extend_from_slice(&base_addr.to_le_bytes()); // p_paddr
    buf.extend_from_slice(&text_size.to_le_bytes()); // p_filesz
    buf.extend_from_slice(&text_size.to_le_bytes()); // p_memsz
    buf.extend_from_slice(&0x1000u64.to_le_bytes()); // p_align

    // Text section
    buf.extend_from_slice(&full_text);

    // .shstrtab section
    buf.extend_from_slice(shstrtab_content);

    // Section header table padding
    let pad = (8 - (buf.len() as u64 % 8)) % 8;
    buf.extend(std::iter::repeat_n(0, pad as usize));

    // Null section header
    buf.extend(std::iter::repeat_n(0, SHDR_SIZE as usize));
    // .shstrtab section header
    write_shdr(
        &mut buf,
        name_offset_u32(shstrtab_content, b".shstrtab"),
        SHT_STRTAB,
        0,
        shstrtab_off,
        shstrtab_size,
    );
    // .text section header
    write_shdr(
        &mut buf,
        name_offset_u32(shstrtab_content, b".text"),
        SHT_PROGBITS,
        SHF_ALLOC | SHF_EXECINSTR,
        text_off,
        text_size,
    );

    Ok(buf)
}

fn write_shdr(buf: &mut Vec<u8>, name: u32, sh_type: u32, flags: u64, offset: u64, size: u64) {
    buf.extend_from_slice(&name.to_le_bytes());
    buf.extend_from_slice(&sh_type.to_le_bytes());
    buf.extend_from_slice(&flags.to_le_bytes());
    buf.extend_from_slice(&0u64.to_le_bytes());
    buf.extend_from_slice(&offset.to_le_bytes());
    buf.extend_from_slice(&size.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u64.to_le_bytes());
    buf.extend_from_slice(&0u64.to_le_bytes());
}

fn name_offset_u32(table: &[u8], name: &[u8]) -> u32 {
    table
        .windows(name.len())
        .position(|w| w == name)
        .unwrap_or(0) as u32
}

fn build_strtab(program: &LirProgram, relocs: &[Reloc]) -> String {
    let mut s = String::new();
    s.push('\0');
    let mut seen = std::collections::HashSet::new();
    
    for func in &program.functions {
        s.push_str(&func.name);
        s.push('\0');
        seen.insert(func.name.clone());
    }
    
    for r in relocs {
        if !seen.contains(&r.target_name) {
            s.push_str(&r.target_name);
            s.push('\0');
            seen.insert(r.target_name.clone());
        }
    }
    s
}

fn build_symtab(program: &LirProgram, strtab: &str, relocs: &[Reloc]) -> Result<Vec<[u8; 24]>, CodegenError> {
    let mut entries = vec![[0u8; 24]; 1]; // Null symbol
    let mut text_off = 0u64;

    let struct_fields: HashMap<String, Vec<String>> = program.structs.iter()
        .map(|s| (s.name.clone(), s.fields.iter().map(|(n, _)| n.clone()).collect()))
        .collect();

    let mut func_labels = std::collections::HashMap::new();
    let mut block_labels = std::collections::HashMap::new();
    let mut block_name_labels = std::collections::HashMap::new();

    // 1. Internal functions
    for func in &program.functions {
        let name_off = strtab.find(&format!("\0{}\0", func.name)).map(|p| p + 1).unwrap_or(0) as u32;
        let mut entry = [0u8; 24];
        entry[0..4].copy_from_slice(&name_off.to_le_bytes());
        entry[4] = STB_GLOBAL | (STT_FUNC << 4);
        entry[6..8].copy_from_slice(&2u16.to_le_bytes()); // Section 2 (.text)
        entry[8..16].copy_from_slice(&text_off.to_le_bytes());
        entries.push(entry);

        let mut dummy_lines = Vec::new();
        let (code, _) = emit_function(func, &struct_fields, &mut func_labels, &mut block_labels, &mut block_name_labels, &mut dummy_lines)?;
        text_off += code.len() as u64;
    }

    // 2. External symbols from relocs
    let mut seen = program.functions.iter().map(|f| f.name.clone()).collect::<std::collections::HashSet<_>>();
    for r in relocs {
        if !seen.contains(&r.target_name) {
            let name_off = strtab.find(&format!("\0{}\0", r.target_name)).map(|p| p + 1).unwrap_or(0) as u32;
            let mut entry = [0u8; 24];
            entry[0..4].copy_from_slice(&name_off.to_le_bytes());
            entry[4] = STB_GLOBAL;
            entry[6..8].copy_from_slice(&0u16.to_le_bytes()); // UNDEFINED
            entries.push(entry);
            seen.insert(r.target_name.clone());
        }
    }
    Ok(entries)
}

fn build_rela_entries(relocs: &[Reloc], symtab: &[[u8; 24]], strtab: &str) -> Vec<[u8; 24]> {
    let mut entries = Vec::new();
    for r in relocs {
        let sym_idx = find_elf_symbol_index(symtab, strtab, &r.target_name);
        let mut entry = [0u8; 24];
        
        // r_offset
        entry[0..8].copy_from_slice(&(r.offset as u64).to_le_bytes());
        
        // r_info: (sym << 32) | type
        // R_X86_64_PLT32 = 4, R_X86_64_PC32 = 2
        let r_type = if r.is_relative { 4u64 } else { 2u64 };
        let r_info = ((sym_idx as u64) << 32) | (r_type as u64);
        entry[8..16].copy_from_slice(&r_info.to_le_bytes());
        
        // r_addend
        entry[16..24].copy_from_slice(&(-4i64).to_le_bytes()); // Typical for PC32/PLT32
        
        entries.push(entry);
    }
    entries
}

fn find_elf_symbol_index(symtab: &[[u8; 24]], strtab: &str, name: &str) -> u32 {
    for (i, entry) in symtab.iter().enumerate() {
        let name_off = u32::from_le_bytes(entry[0..4].try_into().unwrap()) as usize;
        if name_off > 0 {
            let sym_name = read_str(strtab.as_bytes(), name_off);
            if sym_name == name {
                return i as u32;
            }
        }
    }
    0
}

fn read_str(data: &[u8], off: usize) -> String {
    let mut end = off;
    while end < data.len() && data[end] != 0 {
        end += 1;
    }
    String::from_utf8_lossy(&data[off..end]).to_string()
}





