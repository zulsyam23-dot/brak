use std::collections::HashMap;
use crate::parse::*;

use brak_core::Result;
use brak_link_traits::{LinkerOutput, ObjectFile};

pub fn link_elf(objects: &[ObjectFile], entry: &str, base_addr: u64) -> Result<LinkerOutput> {
    if objects.is_empty() {
        return Err("no object files to link".into());
    }

    let parsed: Vec<ParsedElf> = objects.iter().map(|o| parse_elf(&o.data)).collect::<Result<_>>()?;
    let global_syms = build_global_sym_map(&parsed);
    let (merged_text, text_bases) = merge_text(&parsed);
    let merged_text = apply_all_relocs(&parsed, &global_syms, &merged_text, &text_bases, base_addr, 64 + 56)?;

    let entry_offset = find_entry_offset(&parsed, &global_syms, &text_bases, entry);
    let start_stub = build_start_stub(entry_offset);
    let mut full_text = start_stub;
    full_text.extend_from_slice(&merged_text);
    let full_text_size = full_text.len() as u64;

    // ── Collect and merge debug sections ──────────────
    let mut merged_debug: Vec<(String, Vec<u8>)> = Vec::new();
    for p in &parsed {
        for (name, data) in &p.debug_sections {
            let idx = merged_debug.iter().position(|(n, _)| n == name);
            if let Some(idx) = idx {
                merged_debug[idx].1.extend_from_slice(data);
            } else {
                merged_debug.push((name.clone(), data.clone()));
            }
        }
    }
    merged_debug.sort_by(|a, b| a.0.cmp(&b.0));

    // ── Build shstrtab ────────────────────────────────
    let mut shstrtab = Vec::new();
    shstrtab.push(0);
    shstrtab.extend_from_slice(b".shstrtab\0");
    shstrtab.extend_from_slice(b".text\0");
    let mut debug_name_offs: HashMap<String, u32> = HashMap::new();
    for (name, _) in &merged_debug {
        let off = shstrtab.len() as u32;
        shstrtab.extend_from_slice(name.as_bytes());
        shstrtab.push(0);
        debug_name_offs.insert(name.clone(), off);
    }
    let shstrtab_data = shstrtab.clone();
    let shstrtab_size = shstrtab.len() as u64;

    // ── Layout computation ────────────────────────────
    let text_off: u64 = 64 + 56;
    let shstrtab_off = text_off + full_text_size;

    let mut debug_off = shstrtab_off + shstrtab_size;
    let align_pad = (8 - (debug_off % 8)) % 8;
    debug_off += align_pad;

    let mut debug_meta: Vec<(u32, u32)> = Vec::new();
    for (_, data) in &merged_debug {
        let off = debug_off;
        let sz = data.len() as u32;
        debug_meta.push((off as u32, sz));
        debug_off += data.len() as u64;
    }

    let shoff_raw = debug_off;
    let shoff_pad = (8 - (shoff_raw % 8)) % 8;
    let shoff = shoff_raw + shoff_pad;

    let num_sec_headers: u16 = 1 + 2 + merged_debug.len() as u16;

    let mut buf = Vec::new();

    // ── ELF Header (64 bytes) ─────────────────────────
    buf.extend_from_slice(&[0x7f, b'E', b'L', b'F']);
    buf.push(ELFCLASS64);
    buf.push(ELFDATA2LSB);
    buf.push(EV_CURRENT);
    buf.push(0);
    buf.extend_from_slice(&[0u8; 8]);
    write_u16(&mut buf, 2);
    write_u16(&mut buf, EM_X86_64);
    write_u32(&mut buf, 1);
    write_u64(&mut buf, base_addr + text_off);
    write_u64(&mut buf, 64);
    write_u64(&mut buf, shoff);
    write_u32(&mut buf, 0);
    write_u16(&mut buf, 64);
    write_u16(&mut buf, 56);
    write_u16(&mut buf, 1);
    write_u16(&mut buf, 64);
    write_u16(&mut buf, num_sec_headers);
    write_u16(&mut buf, 1);

    // ── Program Header (56 bytes) ─────────────────────
    write_u32(&mut buf, 1);
    write_u32(&mut buf, 5);
    write_u64(&mut buf, text_off);
    write_u64(&mut buf, base_addr);
    write_u64(&mut buf, base_addr);
    write_u64(&mut buf, full_text_size);
    write_u64(&mut buf, full_text_size);
    write_u64(&mut buf, 0x1000);

    // ── Write data ────────────────────────────────────
    buf.extend_from_slice(&full_text);
    buf.extend_from_slice(&shstrtab);
    let pad1 = (8 - (buf.len() as u64 % 8)) % 8;
    buf.extend(std::iter::repeat_n(0, pad1 as usize));

    for (_, data) in &merged_debug {
        buf.extend_from_slice(data);
    }
    let pad2 = (8 - (buf.len() as u64 % 8)) % 8;
    buf.extend(std::iter::repeat_n(0, pad2 as usize));

    // ── Section Headers ───────────────────────────────
    buf.extend(std::iter::repeat_n(0, 64)); // NULL section

    write_name(&mut buf, &shstrtab_data, b".shstrtab");
    write_u32(&mut buf, SHT_STRTAB);
    write_u64(&mut buf, 0); write_u64(&mut buf, 0);
    write_u64(&mut buf, shstrtab_off); write_u64(&mut buf, shstrtab_size);
    write_u32(&mut buf, 0); write_u32(&mut buf, 0);
    write_u64(&mut buf, 0); write_u64(&mut buf, 0);

    write_name(&mut buf, &shstrtab_data, b".text");
    write_u32(&mut buf, SHT_PROGBITS);
    write_u64(&mut buf, 6);
    write_u64(&mut buf, 0);
    write_u64(&mut buf, text_off); write_u64(&mut buf, full_text_size);
    write_u32(&mut buf, 0); write_u32(&mut buf, 0);
    write_u64(&mut buf, 0); write_u64(&mut buf, 0);

    for (idx, (name, _)) in merged_debug.iter().enumerate() {
        let name_off = debug_name_offs[name];
        let (file_off, size) = debug_meta[idx];
        write_u32(&mut buf, name_off);
        write_u32(&mut buf, SHT_PROGBITS);
        write_u64(&mut buf, 0); // flags = not allocated in memory
        write_u64(&mut buf, 0); // addr = 0
        write_u64(&mut buf, file_off as u64);
        write_u64(&mut buf, size as u64);
        write_u32(&mut buf, 0); write_u32(&mut buf, 0);
        write_u64(&mut buf, 0); write_u64(&mut buf, 0);
    }

    Ok(LinkerOutput { data: buf, format: "elf" })
}

fn build_start_stub(entry_offset_in_text: u64) -> Vec<u8> {
    let entry_abs_offset = 16 + entry_offset_in_text;
    let rel32 = (entry_abs_offset.wrapping_sub(7)) as i32;
    let mut stub = vec![
        0x31, 0xff, 0xe8, 0, 0, 0, 0,
        0x89, 0xc7, 0xb8, 0x3c, 0x00, 0x00, 0x00,
        0x0f, 0x05,
    ];
    stub[3..7].copy_from_slice(&rel32.to_le_bytes());
    stub
}

fn write_name(buf: &mut Vec<u8>, table: &[u8], name: &[u8]) {
    let offset = table.windows(name.len()).position(|w| w == name).unwrap_or(0) as u32;
    write_u32(buf, offset);
}
