use crate::x86_64::LineEntry;
use brak_ir_lir::lir::LirProgram;

pub struct DwarfSections {
    pub debug_line: Vec<u8>,
    pub debug_info: Vec<u8>,
    pub debug_abbrev: Vec<u8>,
    pub debug_str: Vec<u8>,
}

pub fn build_dwarf(
    program: &LirProgram,
    text_data: &[u8],
    line_entries: &[LineEntry],
) -> DwarfSections {
    let file_names: Vec<&str> = program.files.iter().map(|f| f.as_str()).collect();
    let producer = "Brak v0.1.0";

    // Compute function byte ranges from line entries
    let func_ranges = compute_func_ranges(program, line_entries, text_data.len());

    let debug_str = build_debug_str(producer, &file_names);
    let debug_abbrev = build_debug_abbrev();
    let debug_line = build_debug_line(text_data.len(), line_entries, &file_names);
    let debug_info = build_debug_info(producer, &file_names, text_data.len(), &func_ranges, 0);

    DwarfSections { debug_line, debug_info, debug_abbrev, debug_str }
}

fn compute_func_ranges(program: &LirProgram, entries: &[LineEntry], text_len: usize) -> Vec<(String, u64, u64)> {
    let mut ranges = Vec::new();
    let mut fi = 0usize;
    let mut prev_end = 0u64;
    for entry in entries {
        if entry.end_sequence {
            let end = entry.offset as u64;
            if fi < program.functions.len() {
                ranges.push((program.functions[fi].name.clone(), prev_end, end));
                fi += 1;
            }
            prev_end = end;
        }
    }
    // Handle any remaining functions that didn't have line entries
    while fi < program.functions.len() {
        ranges.push((program.functions[fi].name.clone(), prev_end, text_len as u64));
        fi += 1;
    }
    ranges
}

fn build_debug_str(producer: &str, files: &[&str]) -> Vec<u8> {
    let mut buf = Vec::new();
    for b in producer.bytes() { buf.push(b); }
    buf.push(0);
    for f in files {
        for b in f.bytes() { buf.push(b); }
        buf.push(0);
    }
    buf
}

fn build_debug_abbrev() -> Vec<u8> {
    let mut buf = Vec::new();

    // Abbrev 1: DW_TAG_compile_unit (DW_CHILDREN_yes)
    buf.extend_from_slice(&1u64.to_leb128());
    buf.extend_from_slice(&0x11u64.to_leb128()); // DW_TAG_compile_unit
    buf.push(1); // DW_CHILDREN_yes
    buf.extend_from_slice(&0x18u64.to_leb128()); // DW_AT_producer
    buf.extend_from_slice(&0x08u64.to_leb128()); // DW_FORM_string
    buf.extend_from_slice(&0x13u64.to_leb128()); // DW_AT_language
    buf.extend_from_slice(&0x0bu64.to_leb128()); // DW_FORM_data1
    buf.extend_from_slice(&0x03u64.to_leb128()); // DW_AT_name
    buf.extend_from_slice(&0x08u64.to_leb128()); // DW_FORM_string
    buf.extend_from_slice(&0x10u64.to_leb128()); // DW_AT_stmt_list
    buf.extend_from_slice(&0x0au64.to_leb128()); // DW_FORM_data4 (section offset)
    buf.extend_from_slice(&0x11u64.to_leb128()); // DW_AT_low_pc
    buf.extend_from_slice(&0x01u64.to_leb128()); // DW_FORM_addr
    buf.extend_from_slice(&0x12u64.to_leb128()); // DW_AT_high_pc
    buf.extend_from_slice(&0x01u64.to_leb128()); // DW_FORM_addr
    buf.push(0); buf.push(0); // end of abbrev

    // Abbrev 2: DW_TAG_subprogram (DW_CHILDREN_no)
    buf.extend_from_slice(&2u64.to_leb128());
    buf.extend_from_slice(&0x2eu64.to_leb128()); // DW_TAG_subprogram
    buf.push(0); // DW_CHILDREN_no
    buf.extend_from_slice(&0x03u64.to_leb128()); // DW_AT_name
    buf.extend_from_slice(&0x08u64.to_leb128()); // DW_FORM_string
    buf.extend_from_slice(&0x11u64.to_leb128()); // DW_AT_low_pc
    buf.extend_from_slice(&0x01u64.to_leb128()); // DW_FORM_addr
    buf.extend_from_slice(&0x12u64.to_leb128()); // DW_AT_high_pc
    buf.extend_from_slice(&0x01u64.to_leb128()); // DW_FORM_addr
    buf.push(0); buf.push(0); // end of abbrev

    buf.push(0); buf.push(0); // end of abbrev table
    buf
}

fn build_debug_line(_text_len: usize, entries: &[LineEntry], files: &[&str]) -> Vec<u8> {
    let mut line_prog = Vec::new();

    let mut prev_line = 0u64;
    let mut prev_addr = 0u64;

    // Group entries by function (detect end_sequence transitions)
    for entry in entries {
        let addr = entry.offset as u64;
        let line = entry.line as u64;
        let col = entry.column as u64;

        if entry.end_sequence {
            // DW_LNE_end_sequence
            line_prog.push(0); // DW_LNE
            line_prog.push(1); // length
            line_prog.push(1); // DW_LNE_end_sequence
            prev_addr = 0;
            prev_line = 0;
            continue;
        }

        if addr > prev_addr {
            // Advance PC
            let addr_delta = addr - prev_addr;
            emit_advance_pc(&mut line_prog, addr_delta);
        }

        if line > prev_line {
            let line_delta = line - prev_line;
            emit_advance_line(&mut line_prog, line_delta as i64);
        } else if line < prev_line && line > 0 {
            let line_delta = prev_line - line;
            emit_advance_line(&mut line_prog, -(line_delta as i64));
        }

        if col > 0 {
            emit_set_column(&mut line_prog, col as u16);
        }

        // DW_LNS_copy
        line_prog.push(1);

        prev_addr = addr;
        prev_line = line;
    }

    // Build full .debug_line section
    let mut buf = Vec::new();

    // Include directories (none)
    let dir_entries = vec![0u8];
    // File names
    let mut file_entries = Vec::new();
    for (_idx, f) in files.iter().enumerate() {
        for b in f.bytes() { file_entries.push(b); }
        file_entries.push(0); // name
        file_entries.extend_from_slice(&0u64.to_le_bytes()); // dir index
        file_entries.extend_from_slice(&0u64.to_le_bytes()); // mtime
        file_entries.extend_from_slice(&0u64.to_le_bytes()); // size
    }
    file_entries.push(0); // end of files

    let header_len = 2 + 4 + 1 + 1 + 1 + 1 + 1 + 12 + dir_entries.len() + file_entries.len();
    let total_len = header_len + line_prog.len();

    // Initial length (4 bytes, excluding itself)
    buf.extend_from_slice(&(total_len as u32).to_le_bytes());
    // Version (2 bytes)
    buf.extend_from_slice(&2u16.to_le_bytes());
    // Prologue length (4 bytes) - length of everything after prologue_length up to line_prog
    let prologue_len = 1 + 1 + 1 + 1 + 1 + 12 + dir_entries.len() + file_entries.len();
    buf.extend_from_slice(&(prologue_len as u32).to_le_bytes());
    // Minimum instruction length
    buf.push(1);
    // Maximum ops per instruction
    buf.push(1);
    // Default is_stmt
    buf.push(1);
    // Line base
    buf.push(0xFDu8 as u8); // -5 as signed byte
    // Line range
    buf.push(14);
    // Opcode base
    buf.push(13);
    // Standard opcode lengths
    buf.extend_from_slice(&[0, 1, 1, 1, 1, 0, 0, 0, 1, 0, 0, 1]);
    // Include directories
    buf.extend_from_slice(&dir_entries);
    // File names
    buf.extend_from_slice(&file_entries);

    // Line number program
    buf.extend_from_slice(&line_prog);

    buf
}

fn build_debug_info(
    producer: &str,
    files: &[&str],
    text_len: usize,
    func_ranges: &[(String, u64, u64)],
    debug_line_offset: u32,
) -> Vec<u8> {
    let abbrev_offset = 0u32;
    let addr_size = 8u8;
    let lang_c89 = 0x01u8;

    let low_pc = 0u64;
    let high_pc = text_len as u64;

    let mut info_body = Vec::new();

    // CU DIE: abbrev 1 (compile_unit)
    info_body.extend_from_slice(&1u64.to_leb128());
    for b in producer.bytes() { info_body.push(b); }
    info_body.push(0);
    info_body.push(lang_c89);
    if let Some(f) = files.first() {
        for b in f.bytes() { info_body.push(b); }
    }
    info_body.push(0);
    info_body.extend_from_slice(&debug_line_offset.to_le_bytes());
    info_body.extend_from_slice(&low_pc.to_le_bytes());
    info_body.extend_from_slice(&high_pc.to_le_bytes());

    // Subprogram DIEs: abbrev 2 (subprogram)
    for (name, start, end) in func_ranges {
        info_body.extend_from_slice(&2u64.to_leb128());
        for b in name.bytes() { info_body.push(b); }
        info_body.push(0);
        info_body.extend_from_slice(&start.to_le_bytes());
        info_body.extend_from_slice(&end.to_le_bytes());
    }

    // Compilation unit header
    let cu_length = 2 + 4 + 1 + info_body.len();
    let mut buf = Vec::new();
    buf.extend_from_slice(&(cu_length as u32).to_le_bytes());
    buf.extend_from_slice(&4u16.to_le_bytes()); // DWARF version 4
    buf.extend_from_slice(&abbrev_offset.to_le_bytes());
    buf.push(addr_size);
    buf.extend_from_slice(&info_body);

    buf
}

fn emit_advance_pc(buf: &mut Vec<u8>, delta: u64) {
    if delta == 0 { return; }
    buf.push(2); // DW_LNS_advance_pc
    buf.extend_from_slice(&delta.to_leb128());
}

fn emit_advance_line(buf: &mut Vec<u8>, delta: i64) {
    if delta == 0 { return; }
    buf.push(3); // DW_LNS_advance_line
    buf.extend_from_slice(&delta.to_leb128());
}

fn emit_set_column(buf: &mut Vec<u8>, col: u16) {
    buf.push(4); // DW_LNS_set_column
    buf.extend_from_slice(&col.to_le_bytes());
}

trait Leb128 {
    fn to_leb128(self) -> Vec<u8>;
}

impl Leb128 for u64 {
    fn to_leb128(self) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut val = self;
        loop {
            let mut byte = (val & 0x7f) as u8;
            val >>= 7;
            if val != 0 { byte |= 0x80; }
            buf.push(byte);
            if val == 0 { break; }
        }
        buf
    }
}

impl Leb128 for i64 {
    fn to_leb128(self) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut val = self;
        loop {
            let mut byte = (val & 0x7f) as u8;
            val >>= 7;
            if !((val == 0 && (byte & 0x40) == 0) || (val == -1 && (byte & 0x40) != 0)) {
                byte |= 0x80;
            }
            buf.push(byte);
            if (val == 0 && (byte & 0x40) == 0) || (val == -1 && (byte & 0x40) != 0) {
                break;
            }
        }
        buf
    }
}
