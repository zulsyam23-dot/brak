use std::process::Command;
use std::path::PathBuf;

fn get_brak_bin() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.push("target");
    p.push("debug");
    p.push("brak.exe");
    p
}

#[test]
fn test_suite_fib() {
    let brak = get_brak_bin();
    let src = "tests/suite/fib.brk";
    let output = "tests/suite/fib.exe";

    // 1. Build
    let status = Command::new(&brak)
        .arg("build")
        .arg(src)
        .arg("--output")
        .arg(output)
        .status()
        .expect("Failed to run brak build");
    
    assert!(status.success());

    // 2. Execute and verify result
    let exec_status = Command::new(format!("./{}", output))
        .status()
        .expect("Failed to execute compiled program");
    
    assert_eq!(exec_status.code().unwrap(), 13);
}

#[test]
fn test_suite_nested() {
    let brak = get_brak_bin();
    let src = "tests/suite/nested.brk";
    let output = "tests/suite/nested.exe";

    // 1. Build
    let status = Command::new(&brak)
        .arg("build")
        .arg(src)
        .arg("--output")
        .arg(output)
        .status()
        .expect("Failed to run brak build");
    
    assert!(status.success());

    // 2. Execute and verify result
    let exec_status = Command::new(format!("./{}", output))
        .status()
        .expect("Failed to execute compiled program");
    
    assert_eq!(exec_status.code().unwrap(), 8);
}

#[test]
fn test_suite_diagnostic_undef() {
    let brak = get_brak_bin();
    let src = "tests/suite/error_undef.brk";

    // Build should fail
    let output = Command::new(&brak)
        .arg("emit-ir")
        .arg(src)
        .arg("--level")
        .arg("hir")
        .output()
        .expect("Failed to run brak emit-ir");
    
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("undefined_variable") || stderr.contains("not found"));
}
