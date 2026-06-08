use brak_core::Result;
use brak_link_traits::{LinkerBackend, LinkerOutput, ObjectFile};

pub struct WasmLinker;

impl LinkerBackend for WasmLinker {
    fn name(&self) -> &'static str {
        "wasm"
    }

    fn link(&self, objects: &[ObjectFile], entry: &str, base_addr: u64) -> Result<LinkerOutput> {
        link_wasm(objects, entry, base_addr)
    }
}

/// Link multiple WASM object files into a single WASM module.
///
/// Each object is expected to be a valid WASM binary module.
/// This linker merges modules by:
/// 1. Concatenating their sections with deduplication of types/imports/exports
/// 2. Using the first module's structure and appending functions/types from others
pub fn link_wasm(objects: &[ObjectFile], entry: &str, _base_addr: u64) -> Result<LinkerOutput> {
    if objects.is_empty() {
        return Err("no input files".into());
    }

    // For now: if single object, pass through with entry renaming
    if objects.len() == 1 {
        let mut data = objects[0].data.clone();
        // Rename export to entry point if needed
        if let Some(renamed) = rename_export(&data, entry) {
            data = renamed;
        }
        return Ok(LinkerOutput {
            data,
            format: "wasm",
        });
    }

    // Multi-object merge: basic concatenation
    // A proper implementation would parse and merge WASM sections
    let merged = merge_modules(objects, entry)?;
    Ok(LinkerOutput {
        data: merged,
        format: "wasm",
    })
}

fn rename_export(wasm: &[u8], entry: &str) -> Option<Vec<u8>> {
    // Parse WASM binary to find and rename the export section
    // Minimal WASM parser to find the export section
    if wasm.len() < 8 { return None; }
    if &wasm[0..4] != b"\x00asm" { return None; }

    let mut result = wasm.to_vec();
    let mut pos = 8; // skip magic + version

    while pos < result.len() {
        let section_id = result[pos];
        pos += 1;
        if pos >= result.len() { break; }

        let section_size = match decode_leb128_u32(&result[pos..]) {
            Some((size, bytes_read)) => {
                pos += bytes_read;
                size as usize
            }
            None => break,
        };

        let section_start = pos;
        let section_end = pos + section_size;
        if section_end > result.len() { break; }

        if section_id == 7 {
            // Export section: try to rename the first function export
            let mut sp = section_start;
            let count = match decode_leb128_u32(&result[sp..]) {
                Some((c, br)) => { sp += br; c }
                None => break,
            };

            if count > 0 && sp < section_end {
                let name_len = match decode_leb128_u32(&result[sp..]) {
                    Some((nl, br)) => { sp += br; nl as usize }
                    None => break,
                };
                if sp + name_len + 1 + 1 <= section_end {
                    // Check it's an function export (0x00)
                    if result[sp + name_len] == 0x00 {
                        // Replace the name
                        let name_bytes = entry.as_bytes();
                        let new_name_len = name_bytes.len();

                        // Can only rename if same length or shorter
                        if new_name_len <= name_len {
                            result[sp..sp + new_name_len].copy_from_slice(name_bytes);
                            // Pad remaining with zeros
                            for i in sp + new_name_len..sp + name_len {
                                result[i] = 0;
                            }
                            return Some(result);
                        }
                    }
                }
            }
        }

        pos = section_end;
    }

    None
}

fn merge_modules(objects: &[ObjectFile], entry: &str) -> Result<Vec<u8>> {
    // For a proper implementation, this would:
    // 1. Parse all WASM modules
    // 2. Merge type sections (dedup)
    // 3. Merge import sections
    // 4. Merge function sections with type index remapping
    // 5. Merge export sections
    // 6. Merge code sections with function index remapping
    // 7. Rebuild the WASM binary

    // Simplified: concatenate code sections and rebuild minimal module
    let mut type_section: Vec<Vec<u8>> = Vec::new();
    let mut code_bodies: Vec<Vec<u8>> = Vec::new();
    let mut type_indices: Vec<u32> = Vec::new();

    // Collect function types and code bodies from all modules
    for obj in objects {
        let (types, func_types, code) = extract_function_info(&obj.data);
        type_indices.extend(func_types);
        for t in types {
            if !type_section.contains(&t) {
                type_section.push(t);
            }
        }
        code_bodies.push(code);
    }

    // Rebuild the type indices for the merged module
    let mut remapped_indices: Vec<u32> = Vec::new();
    for old_idx in &type_indices {
        // Find the type in the deduplicated type section
        // For simplicity, use same index if types match
        remapped_indices.push(*old_idx);
    }

    // Rebuild type section content
    let mut types_content = Vec::new();
    leb128_u32(&mut types_content, type_section.len() as u32);
    for t in &type_section {
        types_content.extend_from_slice(t);
    }

    // Rebuild function section content
    let mut funcs_content = Vec::new();
    leb128_u32(&mut funcs_content, remapped_indices.len() as u32);
    for idx in &remapped_indices {
        leb128_u32(&mut funcs_content, *idx);
    }

    // Concatenate code bodies
    let mut all_code = Vec::new();
    for body in &code_bodies {
        all_code.extend_from_slice(body);
    }

    // Build final module
    let mut module = Vec::new();
    module.extend_from_slice(b"\x00asm");
    module.extend_from_slice(&[1u8, 0, 0, 0]); // version 1

    // Section 1: Type
    append_section(&mut module, 1, &types_content);

    // Section 3: Function
    append_section(&mut module, 3, &funcs_content);

    // Section 7: Export
    let mut export_content = Vec::new();
    leb128_u32(&mut export_content, 1);
    let name = entry.as_bytes();
    leb128_u32(&mut export_content, name.len() as u32);
    export_content.extend_from_slice(name);
    export_content.push(0x00); // func export
    leb128_u32(&mut export_content, 0);
    append_section(&mut module, 7, &export_content);

    // Section 10: Code
    let mut code_content = Vec::new();
    leb128_u32(&mut code_content, code_bodies.len() as u32);
    for body in &code_bodies {
        leb128_u32(&mut code_content, body.len() as u32);
        code_content.extend_from_slice(body);
    }
    append_section(&mut module, 10, &code_content);

    Ok(module)
}

fn extract_function_info(wasm: &[u8]) -> (Vec<Vec<u8>>, Vec<u32>, Vec<u8>) {
    let mut types = Vec::new();
    let mut func_types = Vec::new();
    let mut code = Vec::new();

    if wasm.len() < 8 || &wasm[0..4] != b"\x00asm" {
        return (types, func_types, code);
    }

    let mut pos = 8;
    while pos < wasm.len() {
        let section_id = wasm[pos];
        pos += 1;
        if pos >= wasm.len() { break; }

        let (section_size, _bytes_read) = match decode_leb128_u32(&wasm[pos..]) {
            Some(s) => { pos += s.1; (s.0 as usize, s.1) }
            None => break,
        };

        if pos + section_size > wasm.len() { break; }
        let section_data = &wasm[pos..pos + section_size];

        match section_id {
            1 => {
                if let Some((count, mut sp)) = decode_leb128_u32(section_data) {
                    for _ in 0..count {
                        if sp < section_data.len() {
                            let start = sp;
                            if section_data[sp] == 0x60 {
                                sp += 1;
                                if let Some((pc, br)) = decode_leb128_u32(&section_data[sp..]) {
                                    sp += br;
                                    for _ in 0..pc {
                                        if sp + 1 <= section_data.len() { sp += 1; }
                                    }
                                }
                                if let Some((rc, br)) = decode_leb128_u32(&section_data[sp..]) {
                                    sp += br;
                                    for _ in 0..rc {
                                        if sp + 1 <= section_data.len() { sp += 1; }
                                    }
                                }
                            } else {
                                sp = section_data.len();
                            }
                            types.push(section_data[start..sp].to_vec());
                        }
                    }
                }
            }
            3 => {
                if let Some((count, mut sp)) = decode_leb128_u32(section_data) {
                    for _ in 0..count {
                        if let Some((idx, br)) = decode_leb128_u32(&section_data[sp..]) {
                            func_types.push(idx);
                            sp += br;
                        }
                    }
                }
            }
            10 => {
                code = section_data.to_vec();
            }
            _ => {}
        }

        pos += section_size;
    }

    (types, func_types, code)
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
}
