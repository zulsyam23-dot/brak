use crate::x86_64::LineEntry;
use brak_ir_lir::lir::LirProgram;

/// Build CodeView C13 debug data for COFF `.debug$S` section.
/// Returns raw bytes ready for section content.
pub fn build_codeview(
    program: &LirProgram,
    line_entries: &[LineEntry],
    text_len: usize,
) -> Vec<u8> {
    let func_ranges = compute_func_ranges(program, line_entries, text_len);

    let mut data = Vec::new();

    // C13 signature
    data.extend_from_slice(&4u32.to_le_bytes()); // CV_SIGNATURE_C13

    // ── DEBUG_S_SYMBOLS subsection (type 0xF1) ────────
    let mut syms = Vec::new();
    for (name, start, end) in &func_ranges {
        let len = (*end - *start) as u32;
        emit_gproc32(&mut syms, name, *start as u32, len, 1); // seg=1 (.text)
        emit_s_end(&mut syms);
    }

    // Align subsection data to 4 bytes
    while syms.len() % 4 != 0 {
        syms.push(0);
    }

    // subsection header
    data.extend_from_slice(&0xF1u32.to_le_bytes()); // DEBUG_S_SYMBOLS
    data.extend_from_slice(&(syms.len() as u32).to_le_bytes());
    data.extend_from_slice(&syms);

    // ── DEBUG_S_LINES subsection (type 0xF2) ──────────
    let mut lines_data = build_debug_lines_data(&func_ranges, line_entries);

    if !lines_data.is_empty() {
        // Align subsection data to 4 bytes
        while lines_data.len() % 4 != 0 {
            lines_data.push(0);
        }
        data.extend_from_slice(&0xF2u32.to_le_bytes()); // DEBUG_S_LINES
        data.extend_from_slice(&(lines_data.len() as u32).to_le_bytes());
        data.extend_from_slice(&lines_data);
    }

    data
}

/// Build DEBUG_S_LINES subsection data: line-to-address mappings per function.
fn build_debug_lines_data(
    func_ranges: &[(String, u64, u64)],
    line_entries: &[LineEntry],
) -> Vec<u8> {
    if func_ranges.is_empty() {
        return Vec::new();
    }

    let mut buf = Vec::new();

    for (_, start, end) in func_ranges {
        // Collect non-end_sequence entries within this function's byte range
        let func_lines: Vec<&LineEntry> = line_entries
            .iter()
            .filter(|e| {
                !e.end_sequence
                    && (e.offset as u64) >= *start
                    && (e.offset as u64) < *end
            })
            .collect();

        if func_lines.is_empty() {
            continue;
        }

        let code_len = (*end - *start) as u32;
        let func_start = *start as u32;

        // Block header (16 bytes)
        buf.extend_from_slice(&0u16.to_le_bytes());  // off (padding)
        buf.extend_from_slice(&1u16.to_le_bytes());  // seg = 1 (.text)
        buf.extend_from_slice(&0u32.to_le_bytes());  // flags = 0 (statement)
        buf.extend_from_slice(&code_len.to_le_bytes()); // len
        buf.extend_from_slice(&func_start.to_le_bytes()); // obj

        // Line entries (8 bytes each)
        let last_idx = func_lines.len() - 1;
        for (i, entry) in func_lines.iter().enumerate() {
            let line_off = (entry.offset as u32).wrapping_sub(func_start);
            let mut linenum = entry.line as u32;
            if i == last_idx {
                linenum |= 1 << 24; // EndOfFunction marker
            }
            buf.extend_from_slice(&line_off.to_le_bytes());
            buf.extend_from_slice(&linenum.to_le_bytes());
        }
    }

    buf
}

fn emit_gproc32(buf: &mut Vec<u8>, name: &str, offset: u32, len: u32, seg: u16) {
    let name_bytes = name.as_bytes();
    let name_len = name_bytes.len() + 1; // include null terminator

    let fixed_payload = 2 + 4 * 8 + 2 + 1; // rectyp + 8 u32 fields + seg + flags
    let reclen = fixed_payload + name_len; // reclen = total bytes after reclen field

    buf.extend_from_slice(&(reclen as u16).to_le_bytes());
    buf.extend_from_slice(&0x1110u16.to_le_bytes()); // S_GPROC32
    buf.extend_from_slice(&0u32.to_le_bytes()); // pParent
    buf.extend_from_slice(&0u32.to_le_bytes()); // pEnd
    buf.extend_from_slice(&0u32.to_le_bytes()); // pNext
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes()); // DbgStart
    buf.extend_from_slice(&len.to_le_bytes()); // DbgEnd
    buf.extend_from_slice(&0u32.to_le_bytes()); // typ (void)
    buf.extend_from_slice(&offset.to_le_bytes());
    buf.extend_from_slice(&seg.to_le_bytes());
    buf.push(0); // flags
    buf.extend_from_slice(name_bytes);
    buf.push(0); // null terminator
}

fn emit_s_end(buf: &mut Vec<u8>) {
    let reclen: u16 = 2; // only rectyp follows
    buf.extend_from_slice(&reclen.to_le_bytes());
    buf.extend_from_slice(&0x0006u16.to_le_bytes());
}

fn compute_func_ranges(
    program: &LirProgram,
    entries: &[LineEntry],
    text_len: usize,
) -> Vec<(String, u64, u64)> {
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
    while fi < program.functions.len() {
        ranges.push((program.functions[fi].name.clone(), prev_end, text_len as u64));
        fi += 1;
    }
    ranges
}
