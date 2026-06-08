use crate::parse::*;

use brak_core::Result;
use brak_link_traits::{LinkerOutput, ObjectFile};

const FILE_ALIGN: u64 = 0x200;
const SEC_ALIGN: u64 = 0x1000;
const TEXT_RVA: u64 = SEC_ALIGN;
const TEXT_RAW_OFF: u64 = FILE_ALIGN;

pub fn link_pe(objects: &[ObjectFile], entry: &str, base_addr: u64) -> Result<LinkerOutput> {
    if objects.is_empty() {
        return Err("no object files to link".into());
    }

    let parsed: Vec<ParsedElf> = objects.iter().map(|o| parse_coff(&o.data)).collect::<Result<_>>()?;
    let global_syms = build_global_sym_map(&parsed);
    let (merged_text, text_bases) = merge_text(&parsed);

    // ── Build import data + stub ───────────────────────
    let import_layout = import_data_layout();
    let import_data = build_import_data(&import_layout);
    let entry_offset = find_entry_offset(&parsed, &global_syms, &text_bases, entry);
    let start_stub = build_pe_start_stub(entry_offset, &import_layout);

    // TEXT RVA starts at 0x1000
    // full_text = [import_data] [start_stub] [merged_text]
    let text_start_in_full = (import_data.len() + start_stub.len()) as u64;
    let final_text_body = apply_all_relocs(&parsed, &global_syms, &merged_text, &text_bases, base_addr, TEXT_RVA + text_start_in_full)?;

    let mut full_text = import_data;
    full_text.extend_from_slice(&start_stub);
    full_text.extend_from_slice(&final_text_body);
    let full_text_size = full_text.len() as u64;
    let import_data_len = import_layout.total as u32;

    // ── Merge debug data from all objects ──────────────
    let mut merged_debug = Vec::new();
    for p in &parsed {
        if !p.debug_data.is_empty() {
            merged_debug.extend_from_slice(&p.debug_data);
        }
    }

    // ── PE layout computation ──────────────────────────
    let text_virtual_size = full_text_size;
    let text_raw_size = round_up(full_text_size, FILE_ALIGN);
    let text_raw_off = TEXT_RAW_OFF;

    let has_debug = !merged_debug.is_empty();
    let debug_rva = TEXT_RVA + round_up(text_virtual_size, SEC_ALIGN);
    let debug_raw_off = text_raw_off + text_raw_size;

    // Build RSDS header + C13 data for debug directory
    let mut debug_payload: Vec<u8> = Vec::new();
    let mut rdata_data: Vec<u8> = Vec::new();
    if has_debug {
        // CV_INFO_PDB70 (RSDS)
        debug_payload.extend_from_slice(b"RSDS");
        debug_payload.extend_from_slice(&[0u8; 16]);
        debug_payload.extend_from_slice(&0u32.to_le_bytes());
        debug_payload.extend_from_slice(b"brak.pdb\0");
        debug_payload.extend_from_slice(&merged_debug);

        // IMAGE_DEBUG_DIRECTORY entry (28 bytes)
        let payload_off_in_sec = 28u32;
        let payload_rva = debug_rva as u32 + payload_off_in_sec;
        let payload_raw = debug_raw_off as u32 + payload_off_in_sec;
        let mut debug_dir = Vec::new();
        build_debug_directory_entry(&mut debug_dir, &debug_payload, payload_rva, payload_raw);

        // .rdata = [debug directory] [payload]
        rdata_data.extend_from_slice(&debug_dir);
        rdata_data.extend_from_slice(&debug_payload);
    }

    let debug_raw_size = if has_debug { round_up(rdata_data.len() as u64, FILE_ALIGN) } else { 0 };
    let size_of_headers = TEXT_RAW_OFF;
    let size_of_image = if has_debug {
        debug_rva + round_up(rdata_data.len() as u64, SEC_ALIGN)
    } else {
        TEXT_RVA + round_up(text_virtual_size, SEC_ALIGN)
    };

    let num_sections: u16 = if has_debug { 2 } else { 1 };

    let mut buf = Vec::new();

    // ── DOS HEADER ─────────────────────────────────────
    buf.extend_from_slice(b"MZ");
    buf.resize(60, 0);
    write_u16(&mut buf, 0x40);
    let pe_offset: u32 = 0x40;
    buf.resize(64, 0);
    buf[0x3C..0x40].copy_from_slice(&pe_offset.to_le_bytes());

    // ── PE SIGNATURE ──────────────────────────────────
    buf.extend_from_slice(b"PE\0\0");

    // ── COFF FILE HEADER (20 bytes) ────────────────────
    write_u16(&mut buf, 0x8664);
    write_u16(&mut buf, num_sections);
    write_u32(&mut buf, 0);
    write_u32(&mut buf, 0);
    write_u32(&mut buf, 0);
    write_u16(&mut buf, 240);
    write_u16(&mut buf, 0x0022);

    // ── OPTIONAL HEADER PE32+ (240 bytes) ──────────────
    write_u16(&mut buf, 0x20B);
    write_u8(&mut buf, 14);
    write_u8(&mut buf, 0);
    write_u32(&mut buf, round_up(full_text_size, FILE_ALIGN) as u32);
    write_u32(&mut buf, import_data_len + if has_debug { debug_raw_size as u32 } else { 0 });
    write_u32(&mut buf, 0);
    write_u32(&mut buf, (TEXT_RVA + import_layout.total) as u32);
    write_u32(&mut buf, TEXT_RVA as u32);
    write_u64(&mut buf, base_addr);
    write_u32(&mut buf, SEC_ALIGN as u32);
    write_u32(&mut buf, FILE_ALIGN as u32);
    write_u16(&mut buf, 6);
    write_u16(&mut buf, 0);
    write_u16(&mut buf, 0);
    write_u16(&mut buf, 0);
    write_u16(&mut buf, 6);
    write_u16(&mut buf, 0);
    write_u32(&mut buf, 0);
    write_u32(&mut buf, size_of_image as u32);
    write_u32(&mut buf, size_of_headers as u32);
    write_u32(&mut buf, 0);
    write_u16(&mut buf, 3);
    write_u16(&mut buf, 0);
    write_u64(&mut buf, 0x100000);
    write_u64(&mut buf, 0x1000);
    write_u64(&mut buf, 0x100000);
    write_u64(&mut buf, 0x1000);
    write_u32(&mut buf, 0);
    write_u32(&mut buf, 16);

    // Data directories (16 x 8 bytes)
    for i in 0..16 {
        match i {
            1 => {
                write_u32(&mut buf, TEXT_RVA as u32);
                write_u32(&mut buf, import_data_len as u32);
            }
            6 if has_debug => {
                write_u32(&mut buf, debug_rva as u32);
                write_u32(&mut buf, 28); // one IMAGE_DEBUG_DIRECTORY entry
            }
            _ => {
                write_u32(&mut buf, 0);
                write_u32(&mut buf, 0);
            }
        }
    }

    // ── SECTION HEADER (.text) ─────────────────────────
    buf.extend_from_slice(b".text\0\0\0");
    write_u32(&mut buf, text_virtual_size as u32);
    write_u32(&mut buf, TEXT_RVA as u32);
    write_u32(&mut buf, text_raw_size as u32);
    write_u32(&mut buf, text_raw_off as u32);
    write_u32(&mut buf, 0);
    write_u32(&mut buf, 0);
    write_u16(&mut buf, 0);
    write_u16(&mut buf, 0);
    write_u32(&mut buf, 0xE0000020);

    // ── SECTION HEADER (.rdata) ────────────────────────
    if has_debug {
        buf.extend_from_slice(b".rdata\0\0");
        write_u32(&mut buf, rdata_data.len() as u32);
        write_u32(&mut buf, debug_rva as u32);
        write_u32(&mut buf, debug_raw_size as u32);
        write_u32(&mut buf, debug_raw_off as u32);
        write_u32(&mut buf, 0);
        write_u32(&mut buf, 0);
        write_u16(&mut buf, 0);
        write_u16(&mut buf, 0);
        write_u32(&mut buf, 0x40000040); // INITIALIZED_DATA | MEM_READ
    }

    // ── Align to FILE_ALIGN ────────────────────────────
    while buf.len() as u64 % FILE_ALIGN != 0 {
        buf.push(0);
    }

    // ── Write .text data ──────────────────────────────
    buf.extend_from_slice(&full_text);
    while buf.len() as u64 % FILE_ALIGN != 0 {
        buf.push(0);
    }

    // ── Write .rdata data ──────────────────────────────
    if has_debug {
        buf.extend_from_slice(&rdata_data);
        while buf.len() as u64 % FILE_ALIGN != 0 {
            buf.push(0);
        }
    }

    Ok(LinkerOutput { data: buf, format: "pe" })
}

fn build_debug_directory_entry(buf: &mut Vec<u8>, debug_data: &[u8], rva: u32, raw_off: u32) {
    write_u32(buf, 0); // Characteristics
    write_u32(buf, 0); // TimeDateStamp
    write_u16(buf, 0); // MajorVersion
    write_u16(buf, 0); // MinorVersion
    write_u32(buf, 2); // Type = IMAGE_DEBUG_TYPE_CODEVIEW
    write_u32(buf, debug_data.len() as u32); // SizeOfData
    write_u32(buf, rva); // AddressOfRawData (RVA)
    write_u32(buf, raw_off); // PointerToRawData
}

// ── Import data layout ─────────────────────────────────
struct ImportLayout {
    #[allow(dead_code)]
    desc_size: u64,
    dll_name_off: u64,
    int_off: u64,
    iat_off: u64,
    hint_name_off: u64,
    total: u64,
}

fn import_data_layout() -> ImportLayout {
    let desc_size = 40u64;
    let dll_name_off = desc_size;
    let dll_name_len = 13u64;
    let int_off = dll_name_off + dll_name_len;
    let iat_off = int_off + 16;
    let hint_name_off = iat_off + 16;
    let total = hint_name_off + 15;
    ImportLayout { desc_size, dll_name_off, int_off, iat_off, hint_name_off, total }
}

fn build_import_data(layout: &ImportLayout) -> Vec<u8> {
    let dll_name_rva = TEXT_RVA + layout.dll_name_off;
    let int_rva = TEXT_RVA + layout.int_off;
    let iat_rva = TEXT_RVA + layout.iat_off;
    let hint_name_rva = TEXT_RVA + layout.hint_name_off;

    let mut data = Vec::new();
    write_u32(&mut data, int_rva as u32);
    write_u32(&mut data, 0);
    write_u32(&mut data, 0);
    write_u32(&mut data, dll_name_rva as u32);
    write_u32(&mut data, iat_rva as u32);
    write_u32(&mut data, 0);
    write_u32(&mut data, 0);
    write_u32(&mut data, 0);
    write_u32(&mut data, 0);
    write_u32(&mut data, 0);
    data.extend_from_slice(b"kernel32.dll\0");
    write_u64(&mut data, hint_name_rva);
    write_u64(&mut data, 0);
    write_u64(&mut data, hint_name_rva);
    write_u64(&mut data, 0);
    write_u16(&mut data, 0);
    data.extend_from_slice(b"ExitProcess\0\0");
    assert_eq!(data.len() as u64, layout.total);
    data
}

fn build_pe_start_stub(entry_offset_in_text: u64, layout: &ImportLayout) -> Vec<u8> {
    let start_off = layout.total;
    let call_off = start_off + 4;
    let entry_abs = TEXT_RVA + start_off + 21 + entry_offset_in_text;
    let call_rel32 = (entry_abs as i64).wrapping_sub((TEXT_RVA + call_off + 5) as i64) as i32;
    let iat_rva = TEXT_RVA + layout.iat_off;
    let jmp_off = start_off + 4 + 5 + 2 + 4;
    let jmp_rip = TEXT_RVA + jmp_off + 6;
    let jmp_disp = (iat_rva as i64).wrapping_sub(jmp_rip as i64) as i32;

    let mut stub = vec![
        0x48, 0x83, 0xec, 0x28,
        0xe8, 0, 0, 0, 0,
        0x89, 0xc1,
        0x48, 0x83, 0xc4, 0x28,
        0xff, 0x25, 0, 0, 0, 0,
    ];
    stub[5..9].copy_from_slice(&call_rel32.to_le_bytes());
    stub[17..21].copy_from_slice(&jmp_disp.to_le_bytes());
    stub
}

fn round_up(v: u64, align: u64) -> u64 {
    (v + align - 1) & !(align - 1)
}
