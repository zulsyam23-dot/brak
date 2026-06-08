use std::path::{Path, PathBuf};
use std::fs;
use colored::*;
use similar::{ChangeTag, TextDiff};
use serde::Serialize;
use brak_core::{Result, Diagnostics, Severity};

pub struct SnapshotTester {
    snapshot_dir: PathBuf,
    update: bool,
}

pub struct DiagnosticTester;

impl DiagnosticTester {
    pub fn assert_has_error(diags: &Diagnostics, expected_msg: &str) -> Result<()> {
        let found = diags.entries.iter().any(|d| 
            d.severity == Severity::Error && d.message.contains(expected_msg)
        );

        if found {
            println!("{} diagnostic error containing: {}", "Verified".green(), expected_msg);
            Ok(())
        } else {
            println!("{} expected error containing: '{}', but not found in:", "Error".red(), expected_msg);
            println!("{}", diags);
            Err(format!("Expected error '{}' not found", expected_msg).into())
        }
    }

    pub fn assert_has_warning(diags: &Diagnostics, expected_msg: &str) -> Result<()> {
        let found = diags.entries.iter().any(|d| 
            d.severity == Severity::Warning && d.message.contains(expected_msg)
        );

        if found {
            println!("{} diagnostic warning containing: {}", "Verified".green(), expected_msg);
            Ok(())
        } else {
            println!("{} expected warning containing: '{}', but not found", "Error".red(), expected_msg);
            Err(format!("Expected warning '{}' not found", expected_msg).into())
        }
    }
}

pub struct ExecutionTester;

impl ExecutionTester {
    pub fn assert_output(exe_path: &Path, expected_output: &str) -> Result<()> {
        let output = std::process::Command::new(exe_path)
            .output()
            .map_err(|e| format!("Failed to execute {:?}: {}", exe_path, e))?;

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        
        if stdout == expected_output.trim() {
            println!("{} execution output: {}", "Verified".green(), stdout);
            Ok(())
        } else {
            println!("{} execution output mismatch!", "Error".red());
            println!("  Expected: '{}'", expected_output);
            println!("  Actual:   '{}'", stdout);
            Err(format!("Execution output mismatch for {:?}", exe_path).into())
        }
    }
}

impl SnapshotTester {
    pub fn new<P: AsRef<Path>>(snapshot_dir: P, update: bool) -> Self {
        Self {
            snapshot_dir: snapshot_dir.as_ref().to_path_buf(),
            update,
        }
    }

    pub fn assert_snapshot<T: Serialize>(&self, name: &str, ir: &T) -> Result<()> {
        let yaml = serde_yaml::to_string(ir).map_err(|e| format!("Failed to serialize IR: {}", e))?;
        let snapshot_path = self.snapshot_dir.join(format!("{}.yaml", name));

        if !self.snapshot_dir.exists() {
            fs::create_dir_all(&self.snapshot_dir)?;
        }

        if self.update || !snapshot_path.exists() {
            fs::write(&snapshot_path, &yaml)?;
            println!("{} snapshot: {}", "Updated".green(), name);
            return Ok(());
        }

        let existing_yaml = fs::read_to_string(&snapshot_path)?;

        if yaml != existing_yaml {
            println!("{} mismatch for snapshot: {}", "Error".red(), name);
            self.print_diff(&existing_yaml, &yaml);
            return Err(format!("Snapshot mismatch for {}", name).into());
        }

        println!("{} snapshot: {}", "Verified".green(), name);
        Ok(())
    }

    fn print_diff(&self, old: &str, new: &str) {
        let diff = TextDiff::from_lines(old, new);
        for change in diff.iter_all_changes() {
            let (sign, color) = match change.tag() {
                ChangeTag::Delete => ("-", Color::Red),
                ChangeTag::Insert => ("+", Color::Green),
                ChangeTag::Equal => (" ", Color::White),
            };
            print!(
                "{}{}",
                sign.color(color),
                change.to_string().color(color)
            );
        }
    }
}
