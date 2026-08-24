use serde::{Serialize, Deserialize};
use brak_ir_ast::ast::Type as BrakType;
use brak_ir_hir::hir::{HirProgram, HirType, HirItem};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ForeignType {
    // C Types
    Int8,
    Int16,
    Int32,
    Int64,
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    Float,
    Double,
    Void,
    Ptr(Box<ForeignType>),
    // Python/Other dynamic types (placeholder)
    Any,
}

pub struct PolyglotBridge;

impl PolyglotBridge {
    pub fn brak_to_c(brak_ty: &BrakType) -> ForeignType {
        match brak_ty {
            BrakType::I32 => ForeignType::Int32,
            BrakType::I64 => ForeignType::Int64,
            BrakType::F32 => ForeignType::Float,
            BrakType::F64 => ForeignType::Double,
            BrakType::Bool => ForeignType::Int8, // C doesn't have native bool, usually int
            BrakType::String => ForeignType::Ptr(Box::new(ForeignType::Int8)), // char*
            BrakType::Void => ForeignType::Void,
            BrakType::Named(_) => ForeignType::Any,
            BrakType::Ptr(t) => ForeignType::Ptr(Box::new(Self::brak_to_c(t))),
            BrakType::Ref(t) => ForeignType::Ptr(Box::new(Self::brak_to_c(t))),
            BrakType::Array(t, _) => ForeignType::Ptr(Box::new(Self::brak_to_c(t))),
            BrakType::Slice(t) => ForeignType::Ptr(Box::new(Self::brak_to_c(t))),
            BrakType::Fn(_, _) => ForeignType::Ptr(Box::new(ForeignType::Void)),
        }
    }

    pub fn hir_to_c(hir_ty: &HirType) -> ForeignType {
        match hir_ty {
            HirType::I32 => ForeignType::Int32,
            HirType::I64 => ForeignType::Int64,
            HirType::F32 => ForeignType::Float,
            HirType::F64 => ForeignType::Double,
            HirType::Bool => ForeignType::Int8,
            HirType::String => ForeignType::Ptr(Box::new(ForeignType::Int8)),
            HirType::Void => ForeignType::Void,
            HirType::Named(_) => ForeignType::Any,
            HirType::Ptr(t) => ForeignType::Ptr(Box::new(Self::hir_to_c(t))),
            HirType::Ref(t) => ForeignType::Ptr(Box::new(Self::hir_to_c(t))),
            HirType::Array(t, _) => ForeignType::Ptr(Box::new(Self::hir_to_c(t))),
            HirType::Slice(t) => ForeignType::Ptr(Box::new(Self::hir_to_c(t))),
            HirType::Fn(_, _) => ForeignType::Ptr(Box::new(ForeignType::Void)),
        }
    }

    pub fn c_to_brak(foreign_ty: &ForeignType) -> Option<BrakType> {
        match foreign_ty {
            ForeignType::Int32 => Some(BrakType::I32),
            ForeignType::Int64 => Some(BrakType::I64),
            ForeignType::Float => Some(BrakType::F32),
            ForeignType::Double => Some(BrakType::F64),
            ForeignType::Void => Some(BrakType::Void),
            _ => None, // Not all C types map directly back to Brak core types
        }
    }

    pub fn extract_bindings(program: &HirProgram) -> Vec<FfiBinding> {
        let mut bindings = vec![];
        for item in &program.items {
            if let HirItem::Function(f) = item {
                // By default, we export all functions to C for now
                // In the future, we might only export functions marked with some attribute
                bindings.push(FfiBinding {
                    function_name: f.name.clone(),
                    return_type: Self::hir_to_c(&f.ret_ty),
                    parameters: f.params.iter()
                        .map(|p| (p.name.clone(), Self::hir_to_c(&p.ty)))
                        .collect(),
                });
            }
        }
        bindings
    }
}

pub struct FfiBinding {
    pub function_name: String,
    pub return_type: ForeignType,
    pub parameters: Vec<(String, ForeignType)>,
}

impl FfiBinding {
    pub fn to_c_declaration(&self) -> String {
        let params: Vec<String> = self.parameters.iter()
            .map(|(name, ty)| format!("{} {}", Self::type_to_c_string(ty), name))
            .collect();
        
        format!("{} {}({});", 
            Self::type_to_c_string(&self.return_type),
            self.function_name,
            if params.is_empty() { "void".to_string() } else { params.join(", ") }
        )
    }

    fn type_to_c_string(ty: &ForeignType) -> String {
        match ty {
            ForeignType::Int8 => "int8_t".to_string(),
            ForeignType::Int16 => "int16_t".to_string(),
            ForeignType::Int32 => "int32_t".to_string(),
            ForeignType::Int64 => "int64_t".to_string(),
            ForeignType::UInt8 => "uint8_t".to_string(),
            ForeignType::UInt16 => "uint16_t".to_string(),
            ForeignType::UInt32 => "uint32_t".to_string(),
            ForeignType::UInt64 => "uint64_t".to_string(),
            ForeignType::Float => "float".to_string(),
            ForeignType::Double => "double".to_string(),
            ForeignType::Void => "void".to_string(),
            ForeignType::Ptr(inner) => format!("{}*", Self::type_to_c_string(inner)),
            ForeignType::Any => "void*".to_string(),
        }
    }
}

pub struct CHeaderGenerator;

impl CHeaderGenerator {
    pub fn generate_string(bindings: &[FfiBinding]) -> String {
        let mut header = String::new();
        header.push_str("/* Automatically generated by Brak Polyglot */\n\n");
        header.push_str("#ifndef BRAK_GENERATED_H\n");
        header.push_str("#define BRAK_GENERATED_H\n\n");
        header.push_str("#include <stdint.h>\n#include <stdbool.h>\n\n");
        
        for binding in bindings {
            header.push_str(&binding.to_c_declaration());
            header.push('\n');
        }
        
        header.push_str("\n#endif // BRAK_GENERATED_H\n");
        header
    }

    pub fn generate_file(path: &std::path::Path, bindings: &[FfiBinding]) -> std::io::Result<()> {
        let header = Self::generate_string(bindings);
        std::fs::write(path, header)
    }
}

pub struct PyO3Generator;

impl PyO3Generator {
    pub fn generate_string(module_name: &str, bindings: &[FfiBinding]) -> String {
        let mut s = String::new();
        s.push_str("use pyo3::prelude::*;\n\n");
        
        s.push_str("/* External declarations from Brak object */\n");
        s.push_str("extern \"C\" {\n");
        for b in bindings {
            s.push_str(&format!("    #[link_name = \"{}\"]\n", b.function_name));
            s.push_str(&format!("    fn brak_{}(", b.function_name));
            let params: Vec<String> = b.parameters.iter()
                .map(|(_, ty)| Self::type_to_rust_ffi_string(ty))
                .collect();
            s.push_str(&params.join(", "));
            s.push_str(") -> ");
            s.push_str(&Self::type_to_rust_ffi_string(&b.return_type));
            s.push_str(";\n");
        }
        s.push_str("}\n\n");

        for b in bindings {
            s.push_str(&format!("#[pyfunction]\nfn {}(", b.function_name));
            let params: Vec<String> = b.parameters.iter()
                .map(|(name, ty)| format!("{}: {}", name, Self::type_to_pyo3_string(ty)))
                .collect();
            s.push_str(&params.join(", "));
            s.push_str(") -> PyResult<");
            s.push_str(&Self::type_to_pyo3_string(&b.return_type));
            s.push_str("> {\n");
            s.push_str(&format!("    unsafe {{ Ok(brak_{}(" , b.function_name));
            let args: Vec<String> = b.parameters.iter()
                .map(|(name, _)| name.clone())
                .collect();
            s.push_str(&args.join(", "));
            s.push_str(")) }\n}\n\n");
        }

        s.push_str(&format!("#[pymodule]\nfn {}(_py: Python, m: &PyModule) -> PyResult<()> {{\n", module_name));
        for b in bindings {
            s.push_str(&format!("    m.add_function(wrap_pyfunction!({}, m)?)?;\n", b.function_name));
        }
        s.push_str("    Ok(())\n}\n");
        s
    }

    /// Generate a complete Cargo project for a Python extension module.
    ///
    /// BUG-M07: previously returned Cargo.toml and Rust source concatenated
    /// into one string with a comment separator — unusable as either file.
    /// Now writes `Cargo.toml` and `src/lib.rs` into `dir` separately.
    pub fn generate_project(
        dir: &std::path::Path,
        module_name: &str,
        bindings: &[FfiBinding],
        lib_name: &str,
    ) -> Result<(), std::io::Error> {
        let src_dir = dir.join("src");
        std::fs::create_dir_all(&src_dir)?;

        let cargo_toml = format!(
            "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [lib]\nname = \"{}\"\ncrate-type = [\"cdylib\"]\n\n\
             [dependencies]\npyo3 = {{ version = \"0.23\", features = [\"extension-module\"] }}\n\n\
             [build-dependencies]\ncc = \"1\"\n",
            module_name, module_name
        );
        std::fs::write(dir.join("Cargo.toml"), cargo_toml)?;

        let lib_rs = format!(
            "// Generated by Brak Polyglot — Python bindings for {}\n\n{}",
            lib_name,
            Self::generate_string(module_name, bindings)
        );
        std::fs::write(src_dir.join("lib.rs"), lib_rs)?;
        Ok(())
    }

    /// Generate a `pyproject.toml` for the Python package
    pub fn generate_pyproject(module_name: &str) -> String {
        format!(
            r#"[build-system]
requires = ["maturin>=1.0"]
build-backend = "maturin"

[project]
name = "{name}"
version = "0.1.0"
requires-python = ">=3.8"
"#,
            name = module_name
        )
    }

    fn type_to_rust_ffi_string(ty: &ForeignType) -> String {
        match ty {
            ForeignType::Int8 => "i8".to_string(),
            ForeignType::Int16 => "i16".to_string(),
            ForeignType::Int32 => "i32".to_string(),
            ForeignType::Int64 => "i64".to_string(),
            ForeignType::UInt8 => "u8".to_string(),
            ForeignType::UInt16 => "u16".to_string(),
            ForeignType::UInt32 => "u32".to_string(),
            ForeignType::UInt64 => "u64".to_string(),
            ForeignType::Float => "f32".to_string(),
            ForeignType::Double => "f64".to_string(),
            ForeignType::Void => "()".to_string(),
            ForeignType::Ptr(inner) => format!("*mut {}", Self::type_to_rust_ffi_string(inner)),
            ForeignType::Any => "*mut std::ffi::c_void".to_string(),
        }
    }

    fn type_to_pyo3_string(ty: &ForeignType) -> String {
        match ty {
            ForeignType::Int8 | ForeignType::Int16 | ForeignType::Int32 | ForeignType::Int64 => "i64".to_string(),
            ForeignType::UInt8 | ForeignType::UInt16 | ForeignType::UInt32 | ForeignType::UInt64 => "u64".to_string(),
            ForeignType::Float | ForeignType::Double => "f64".to_string(),
            ForeignType::Void => "()".to_string(),
            ForeignType::Ptr(_) | ForeignType::Any => "usize".to_string(),
        }
    }
}
