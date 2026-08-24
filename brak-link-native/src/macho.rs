use crate::parse::*;

use brak_core::Result;
use brak_link_traits::{LinkerOutput, ObjectFile};

pub fn link_macho(objects: &[ObjectFile], entry: &str, base_addr: u64) -> Result<LinkerOutput> {
    if objects.is_empty() {
        return Err("no object files to link".into());
    }

    let parsed: Vec<ParsedElf> = objects.iter().map(|o| parse_macho(&o.data)).collect::<Result<_>>()?;
    let global_syms = build_global_sym_map(&parsed);
    let (merged_text, text_bases) = merge_text(&parsed);
    let merged_text = apply_all_relocs(&parsed, &global_syms, &merged_text, &text_bases, base_addr, 0)?;

    // ── Build _start stub ──────────────────────────────
    let entry_offset = find_entry_offset(&parsed, &global_syms, &text_bases, entry)?;
    let start_stub = build_macho_start_stub(entry_offset);
    let mut full_text = start_stub;
    full_text.extend_from_slice(&merged_text);
    let text_size = full_text.len() as u64;

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
    let ndwarf = merged_debug.len() as u32;
    let has_dwarf = ndwarf > 0;
    let dwarf_data_size: u64 = merged_debug.iter().map(|(_, d)| d.len() as u64).sum();

    // ── Layout computation ────────────────────────────
    // mach_header_64 (32) + LC_SEGMENT_64 __TEXT (152) + LC_SEGMENT_64 __DWARF (72 + N*80) + LC_MAIN (24)
    let dwarf_lc_size: u64 = if has_dwarf { 72 + (ndwarf as u64) * 80 } else { 0 };
    let hdrs_size: u64 = 32 + 152 + dwarf_lc_size + 24;
    let text_file_off = hdrs_size;
    let dwarf_file_off = text_file_off + text_size;
    let ncmds: u32 = if has_dwarf { 3 } else { 2 };
    let sizeofcmds: u32 = (152 + dwarf_lc_size + 24) as u32;

    let mut buf = Vec::new();

    // ── mach_header_64 (32 bytes) ──────────────────────
    write_u32(&mut buf, 0xFEEDFACF);
    write_u32(&mut buf, 0x01000007);
    write_u32(&mut buf, 3);
    write_u32(&mut buf, 2); // MH_EXECUTE
    write_u32(&mut buf, ncmds);
    write_u32(&mut buf, sizeofcmds);
    write_u32(&mut buf, 0);
    write_u32(&mut buf, 0);

    // ── LC_SEGMENT_64 (__TEXT) ─────────────────────────
    write_u32(&mut buf, 0x19);
    write_u32(&mut buf, 152);
    let seg_text = b"__TEXT\0\0\0\0\0\0\0\0\0\0";
    buf.extend_from_slice(seg_text);
    write_u64(&mut buf, base_addr);
    write_u64(&mut buf, text_size);
    write_u64(&mut buf, text_file_off);
    write_u64(&mut buf, text_size);
    write_u32(&mut buf, 7);
    write_u32(&mut buf, 5);
    write_u32(&mut buf, 1);
    write_u32(&mut buf, 0);

    let sec_text = b"__text\0\0\0\0\0\0\0\0\0\0";
    buf.extend_from_slice(sec_text);
    buf.extend_from_slice(seg_text);
    write_u64(&mut buf, base_addr);
    write_u64(&mut buf, text_size);
    write_u32(&mut buf, text_file_off as u32);
    write_u32(&mut buf, 0);
    write_u32(&mut buf, 0);
    write_u32(&mut buf, 0);
    write_u32(&mut buf, 0x80000400);
    write_u32(&mut buf, 0);
    write_u32(&mut buf, 0);
    write_u32(&mut buf, 0);

    // ── LC_SEGMENT_64 (__DWARF) ──────────────────────
    if has_dwarf {
        write_u32(&mut buf, 0x19);
        write_u32(&mut buf, dwarf_lc_size as u32);
        let seg_dwarf = b"__DWARF\0\0\0\0\0\0\0\0\0\0";
        buf.extend_from_slice(seg_dwarf);
        write_u64(&mut buf, base_addr + text_size);
        write_u64(&mut buf, dwarf_data_size);
        write_u64(&mut buf, dwarf_file_off);
        write_u64(&mut buf, dwarf_data_size);
        write_u32(&mut buf, 0); // maxprot
        write_u32(&mut buf, 0); // initprot
        write_u32(&mut buf, ndwarf);
        write_u32(&mut buf, 0);

        let mut sec_file_off = dwarf_file_off;
        for (name, data) in &merged_debug {
            let mut sectname = [0u8; 16];
            let nb = name.as_bytes();
            let copy_len = nb.len().min(16);
            sectname[..copy_len].copy_from_slice(&nb[..copy_len]);
            buf.extend_from_slice(&sectname);
            buf.extend_from_slice(seg_dwarf);
            write_u64(&mut buf, base_addr + text_size + (sec_file_off - dwarf_file_off));
            write_u64(&mut buf, data.len() as u64);
            write_u32(&mut buf, sec_file_off as u32);
            write_u32(&mut buf, 0);
            write_u32(&mut buf, 0);
            write_u32(&mut buf, 0);
            write_u32(&mut buf, 0);
            write_u32(&mut buf, 0);
            write_u32(&mut buf, 0);
            write_u32(&mut buf, 0);
            sec_file_off += data.len() as u64;
        }
    }

    // ── LC_MAIN ────────────────────────────────────────
    write_u32(&mut buf, 0x28);
    write_u32(&mut buf, 24);
    write_u64(&mut buf, text_file_off); // entryoff (file offset)
    write_u64(&mut buf, 0);

    // ── __text section data ────────────────────────────
    buf.extend_from_slice(&full_text);

    // ── __DWARF section data ─────────────────────────
    for (_, data) in &merged_debug {
        buf.extend_from_slice(data);
    }

    Ok(LinkerOutput { data: buf, format: "macho" })
}

fn build_macho_start_stub(entry_offset_in_text: u64) -> Vec<u8> {
    let call_off = 2u64;
    let entry_abs = 16 + entry_offset_in_text;
    let call_rel32 = (entry_abs as i64).wrapping_sub((call_off + 5) as i64) as i32;

    let mut stub = vec![
        0x31, 0xff,
        0xe8, 0, 0, 0, 0,
        0x89, 0xc7,
        0xb8, 0x01, 0x00, 0x00, 0x20,
        0x0f, 0x05,
    ];
    stub[3..7].copy_from_slice(&call_rel32.to_le_bytes());
    stub
}

