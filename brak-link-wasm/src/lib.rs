use std::collections::HashMap;
use brak_core::Result;
use brak_link_traits::{LinkerBackend, LinkerOutput, ObjectFile};

/// BUG-H01 FIXED: merges WASM modules with real type-index remapping, parses
/// Code sections per-function-body (no more double nesting), preserves
/// Memory/Global/Data sections, and rebuilds exports with correct LEB128
/// lengths.
///
/// Known limitation: the exported entry points at function index 0 — member
/// modules carry no name→function mapping to do better without full parsing.
pub struct WasmLinker;

impl LinkerBackend for WasmLinker {
    fn name(&self) -> &'static str {
        "wasm"
    }

    fn link(&self, objects: &[ObjectFile], entry: &str, base_addr: u64) -> Result<LinkerOutput> {
        let _ = base_addr;
        link_wasm(objects, entry)
    }
}

pub fn link_wasm(objects: &[ObjectFile], entry: &str) -> Result<LinkerOutput> {
    if objects.is_empty() {
        return Err("no input files".into());
    }

    for obj in objects {
        if obj.data.len() < 8 || &obj.data[0..4] != b"\x00asm" {
            return Err(format!("'{}' is not a valid WASM binary module", obj.name).into());
        }
    }

    let data = if objects.len() == 1 {
        rename_export(&objects[0].data, entry).unwrap_or_else(|| objects[0].data.clone())
    } else {
        merge_modules(objects, entry)?
    };

    Ok(LinkerOutput { data, format: "wasm" })
}

// ── Module model ─────────────────────────────────────────────

struct ParsedModule {
    /// Section 1 entries: full type vectors (0x60 ...)
    types: Vec<Vec<u8>>,
    /// Section 3 entries: type index per declared function
    func_type_indices: Vec<u32>,
    /// Section 10 entries: individual function bodies (with their locals decls)
    bodies: Vec<Vec<u8>>,
    /// Raw contents of preserved non-code sections by id (5=Memory, 6=Global,
    /// 11=Data), first occurrence wins on duplicates.
    other_sections: HashMap<u8, Vec<u8>>,
}

fn parse_module(wasm: &[u8]) -> Result<ParsedModule> {
    let mut m = ParsedModule {
        types: vec![], func_type_indices: vec![], bodies: vec![],
        other_sections: HashMap::new(),
    };
    let mut pos = 8; // magic + version

    while pos < wasm.len() {
        let section_id = wasm[pos];
        pos += 1;
        let (section_size, br) = decode_leb128_u32(&wasm[pos..])
            .ok_or("corrupt WASM: bad section size")?;
        pos += br;
        let end = pos + section_size as usize;
        if end > wasm.len() { return Err("corrupt WASM: section overruns file".into()); }
        let sec = &wasm[pos..end];
        pos = end;

        match section_id {
            1 => {
                let (count, mut sp) = decode_leb128_u32(sec)
                    .ok_or("corrupt type section")?;
                for _ in 0..count {
                    let (len, br) = decode_leb128_u32(&sec[sp..]).ok_or("corrupt type entry")?;
                    sp += br;
                    let ty_end = sp + len as usize;
                    if ty_end > sec.len() { return Err("corrupt type entry".into()); }
                    // Intern only func types (0x60); others copied verbatim.
                    m.types.push(sec[sp..ty_end].to_vec());
                    sp = ty_end;
                }
            }
            3 => {
                let (count, mut sp) = decode_leb128_u32(sec).ok_or("corrupt function section")?;
                for _ in 0..count {
                    let (idx, br) = decode_leb128_u32(&sec[sp..]).ok_or("corrupt function entry")?;
                    m.func_type_indices.push(idx);
                    sp += br;
                }
            }
            10 => {
                let (count, mut sp) = decode_leb128_u32(sec).ok_or("corrupt code section")?;
                for _ in 0..count {
                    let (len, br) = decode_leb128_u32(&sec[sp..]).ok_or("corrupt code body")?;
                    sp += br;
                    let body_end = sp + len as usize;
                    if body_end > sec.len() { return Err("corrupt code body".into()); }
                    m.bodies.push(sec[sp..body_end].to_vec());
                    sp = body_end;
                }
            }
            5 | 6 | 11 => {
                m.other_sections.entry(section_id).or_insert_with(|| sec.to_vec());
            }
            _ => {} // imports/tables/elems/exports/start are not carried over
        }
    }
    Ok(m)
}

// ── Merging ──────────────────────────────────────────────────

fn merge_modules(objects: &[ObjectFile], entry: &str) -> Result<Vec<u8>> {
    let mut interned: HashMap<Vec<u8>, u32> = HashMap::new();
    let mut merged_types: Vec<Vec<u8>> = Vec::new();
    let mut merged_func_indices: Vec<u32> = Vec::new();
    let mut merged_bodies: Vec<Vec<u8>> = Vec::new();
    let mut other_sections: HashMap<u8, Vec<u8>> = HashMap::new();

    for obj in objects {
        let m = parse_module(&obj.data)?;
        // Real type-index remapping: intern this module's types into the
        // merged set and translate every function-section reference.
        let local_to_merged: Vec<u32> = m.types.iter().map(|t| {
            *interned.entry(t.clone()).or_insert_with(|| {
                merged_types.push(t.clone());
                (merged_types.len() - 1) as u32
            })
        }).collect();

        if m.func_type_indices.len() != m.bodies.len() && !(m.func_type_indices.is_empty() && m.bodies.is_empty()) {
            return Err(format!("module '{}': function/type count mismatch", obj.name).into());
        }
        merged_func_indices.extend(m.func_type_indices.iter().map(|i| local_to_merged[*i as usize]));
        merged_bodies.extend(m.bodies);
        for (id, sec) in &m.other_sections {
            other_sections.entry(*id).or_insert_with(|| sec.clone());
        }
    }

    let mut module = Vec::new();
    module.extend_from_slice(b"\x00asm");
    module.extend_from_slice(&[1u8, 0, 0, 0]); // version 1

    // Canonical section order: 1 Type, 3 Function, 5 Memory, 6 Global,
    // 7 Export, 10 Code, 11 Data.
    if !merged_types.is_empty() {
        let mut c = Vec::new();
        leb128_u32(&mut c, merged_types.len() as u32);
        for t in &merged_types {
            leb128_u32(&mut c, t.len() as u32);
            c.extend_from_slice(t);
        }
        append_section(&mut module, 1, &c);
    }

    if !merged_func_indices.is_empty() {
        let mut c = Vec::new();
        leb128_u32(&mut c, merged_func_indices.len() as u32);
        for idx in &merged_func_indices { leb128_u32(&mut c, *idx); }
        append_section(&mut module, 3, &c);
    }

    if let Some(mem) = other_sections.get(&5) {
        append_section(&mut module, 5, mem);
    }
    if let Some(glob) = other_sections.get(&6) {
        append_section(&mut module, 6, glob);
    }

    // Export the entry point as function 0 (see limitation above).
    let mut export_content = Vec::new();
    leb128_u32(&mut export_content, 1);
    let name = entry.as_bytes();
    leb128_u32(&mut export_content, name.len() as u32);
    export_content.extend_from_slice(name);
    export_content.push(0x00); // func export
    leb128_u32(&mut export_content, 0);
    append_section(&mut module, 7, &export_content);

    if !merged_bodies.is_empty() {
        let mut c = Vec::new();
        leb128_u32(&mut c, merged_bodies.len() as u32);
        for body in &merged_bodies {
            leb128_u32(&mut c, body.len() as u32);
            c.extend_from_slice(body);
        }
        append_section(&mut module, 10, &c);
    }
    if let Some(data_sec) = other_sections.get(&11) {
        append_section(&mut module, 11, data_sec);
    }

    Ok(module)
}

// ── Export renaming ──────────────────────────────────────────

/// Rename the first function export to `entry`. The whole export section is
/// REBUILT (correct LEB128 length), unlike the previous NUL-padding hack.
fn rename_export(wasm: &[u8], entry: &str) -> Option<Vec<u8>> {
    let mut pos = 8;
    while pos < wasm.len() {
        let section_id = wasm[pos];
        pos += 1;
        let (section_size, br) = decode_leb128_u32(&wasm[pos..])?;
        pos += br;
        let end = pos + section_size as usize;
        if end > wasm.len() { break; }

        if section_id == 7 {
            let sec = &wasm[pos..end];
            let (count, mut sp) = decode_leb128_u32(sec)?;
            let mut exports: Vec<(String, u8, u32)> = Vec::new();
            for _ in 0..count {
                let (nl, br) = decode_leb128_u32(&sec[sp..])?;
                sp += br;
                let name = String::from_utf8_lossy(&sec[sp..sp + nl as usize]).to_string();
                sp += nl as usize;
                if sp >= sec.len() { return None; }
                let kind = sec[sp]; sp += 1;
                let (idx, br) = decode_leb128_u32(&sec[sp..])?;
                sp += br;
                exports.push((name, kind, idx));
            }

            // Rewrite the FIRST function export's name.
            let mut rebuilt = Vec::new();
            leb128_u32(&mut rebuilt, count);
            for (i, (n, kind, idx)) in exports.iter().enumerate() {
                let final_name = if i == 0 && *kind == 0x00 {
                    entry.to_string()
                } else {
                    n.clone()
                };
                leb128_u32(&mut rebuilt, final_name.len() as u32);
                rebuilt.extend_from_slice(final_name.as_bytes());
                rebuilt.push(*kind);
                leb128_u32(&mut rebuilt, *idx);
            }

            // Truncate back to BEFORE this section's id+size, then re-emit.
            let header_start = pos - 1 - br;
            let mut out = wasm[..header_start].to_vec();
            out.push(7);
            leb128_u32(&mut out, rebuilt.len() as u32);
            out.extend_from_slice(&rebuilt);
            out.extend_from_slice(&wasm[end..]); // rest unchanged
            return Some(out);
        }
        pos = end;
    }
    None
}

fn append_section(module: &mut Vec<u8>, section_id: u8, content: &[u8]) {
    module.push(section_id);
    leb128_u32(module, content.len() as u32);
    module.extend_from_slice(content);
}

fn leb128_u32(buf: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if value == 0 { break; }
    }
}

fn decode_leb128_u32(buf: &[u8]) -> Option<(u32, usize)> {
    let mut result: u32 = 0;
    let mut shift: u32 = 0;
    let mut bytes_read: usize = 0;

    for &byte in buf {
        bytes_read += 1;
        result |= ((byte & 0x7f) as u32) << shift;
        if byte & 0x80 == 0 {
            return Some((result, bytes_read));
        }
        shift += 7;
        if shift > 28 { return None; }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_module(func_name: &[u8], type_bytes: &[u8], body: &[u8]) -> Vec<u8> {
        let mut m = Vec::new();
        m.extend_from_slice(b"\x00asm\x01\0\0\0");
        // Type section: 1 type (entries are length-prefixed)
        let mut tc = Vec::new();
        leb128_u32(&mut tc, 1);
        leb128_u32(&mut tc, type_bytes.len() as u32);
        tc.extend_from_slice(type_bytes);
        append_section(&mut m, 1, &tc);
        // Function section: 1 func, type 0
        let mut fc = Vec::new();
        leb128_u32(&mut fc, 1);
        leb128_u32(&mut fc, 0);
        append_section(&mut m, 3, &fc);
        // Export section
        let mut ec = Vec::new();
        leb128_u32(&mut ec, 1);
        leb128_u32(&mut ec, func_name.len() as u32);
        ec.extend_from_slice(func_name);
        ec.push(0x00);
        leb128_u32(&mut ec, 0);
        append_section(&mut m, 7, &ec);
        // Code section: 1 body
        let mut cc = Vec::new();
        leb128_u32(&mut cc, 1);
        leb128_u32(&mut cc, body.len() as u32);
        cc.extend_from_slice(body);
        append_section(&mut m, 10, &cc);
        m
    }

    #[test]
    fn test_leb128_roundtrip() {
        let values = [0u32, 1, 42, 127, 128, 255, 65535, 100000, 0x7fffffff];
        for &v in &values {
            let mut buf = Vec::new();
            leb128_u32(&mut buf, v);
            let (decoded, _) = decode_leb128_u32(&buf).unwrap();
            assert_eq!(v, decoded, "LEB128 roundtrip failed for {}", v);
        }
    }

    /// BUG-H01 regression: merging two single-func modules must produce a
    /// well-formed module — deduped types, remapped function indices, and a
    /// Code section whose body sizes match exactly.
    #[test]
    fn test_merge_two_modules() {
        // () -> i64 func type: 0x60 00 01 7e
        let ty = [0x60u8, 0x00, 0x01, 0x7e];
        // body: empty locals + i64.const 7 + end
        let body_a = [0x00u8, 0x42, 0x07, 0x0b];
        let body_b = [0x00u8, 0x42, 0x09, 0x0b];

        let mod_a = test_module(b"a", &ty, &body_a);
        let mod_b = test_module(b"b", &ty, &body_b);

        let objects = vec![
            ObjectFile { name: "a.wasm".into(), data: mod_a },
            ObjectFile { name: "b.wasm".into(), data: mod_b },
        ];
        let out = link_wasm(&objects, "main").unwrap().data;

        // Walk the merged module and validate structure.
        let parsed = parse_module(&out).unwrap();
        assert_eq!(parsed.types.len(), 1, "identical types must dedupe");
        assert_eq!(parsed.func_type_indices, vec![0, 0]);
        assert_eq!(parsed.bodies.len(), 2);
        assert_eq!(parsed.bodies[0], body_a.to_vec());
        assert_eq!(parsed.bodies[1], body_b.to_vec());

        // Code section body sizes must match actual body lengths (no double-nest).
        let mut pos = 8;
        while pos < out.len() {
            let id = out[pos]; pos += 1;
            let (size, br) = decode_leb128_u32(&out[pos..]).unwrap();
            pos += br;
            if id == 10 {
                let (count, mut sp) = decode_leb128_u32(&out[pos..pos + size as usize]).unwrap();
                assert_eq!(count, 2);
                for expected in [&body_a[..], &body_b[..]] {
                    let (blen, br2) = decode_leb128_u32(&out[pos + sp..pos + size as usize]).unwrap();
                    sp += br2;
                    assert_eq!(blen as usize, expected.len());
                    sp += blen as usize;
                }
                break;
            }
            pos += size as usize;
        }
    }

    /// BUG-H01 regression: renaming an export to a LONGER name must produce a
    /// correctly re-encoded export section (no NUL padding).
    #[test]
    fn test_rename_export_longer_name() {
        let ty = [0x60u8, 0x00, 0x01, 0x7e];
        let body = [0x00u8, 0x42, 0x07, 0x0b];
        let m = test_module(b"ab", &ty, &body);
        let renamed = rename_export(&m, "longer_name").expect("renamed");
        // Re-parse: export section must contain exactly the new name.
        eprintln!("orig={:02x?}", m);
        eprintln!("ren={:02x?}", renamed);
        let parsed = parse_module(&renamed).unwrap(); // must not error
        let _ = parsed;
        let needle = b"longer_name";
        assert!(renamed.windows(needle.len()).any(|w| w == needle));
        assert!(!renamed.windows(4).any(|w| w == b"ab\0\0"), "no NUL padding remains");
    }
}
