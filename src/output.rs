use crate::color::{ColorConfig, ColorScheme};

/// Output manager that respects quiet mode and color settings.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Output {
    quiet: bool,
    color: ColorConfig,
}

impl Output {
    /// Create a new Output instance.
    pub fn new(quiet: bool, color: ColorConfig) -> Self {
        Self { quiet, color }
    }

    /// Print mkdir operation.
    pub fn mkdir(&self, path: &std::path::Path, description: Option<&str>) {
        if self.quiet {
            return;
        }
        if self.color.is_enabled() {
            match description {
                Some(desc) => println!(
                    "{}: {} ({})",
                    ColorScheme::operation("Creating"),
                    ColorScheme::path(&path.display().to_string()),
                    desc
                ),
                None => println!(
                    "{}: {}",
                    ColorScheme::operation("Creating"),
                    ColorScheme::path(&path.display().to_string())
                ),
            }
        } else {
            match description {
                Some(desc) => println!("Creating: {} ({desc})", path.display()),
                None => println!("Creating: {}", path.display()),
            }
        }
    }

    /// Print file operation (link or copy).
    fn print_file_op(
        &self,
        op: &str,
        source: &std::path::Path,
        target: &std::path::Path,
        description: Option<&str>,
    ) {
        if self.quiet {
            return;
        }
        let source_str = source.file_name().unwrap_or_default().to_string_lossy();
        let target_str = target.file_name().unwrap_or_default().to_string_lossy();

        if self.color.is_enabled() {
            if source_str == target_str {
                match description {
                    Some(desc) => println!(
                        "{}: {} ({})",
                        ColorScheme::operation(op),
                        ColorScheme::path(&source_str),
                        desc
                    ),
                    None => println!(
                        "{}: {}",
                        ColorScheme::operation(op),
                        ColorScheme::path(&source_str)
                    ),
                }
            } else {
                match description {
                    Some(desc) => println!(
                        "{}: {} → {} ({})",
                        ColorScheme::operation(op),
                        ColorScheme::path(&source_str),
                        ColorScheme::path(&target_str),
                        desc
                    ),
                    None => println!(
                        "{}: {} → {}",
                        ColorScheme::operation(op),
                        ColorScheme::path(&source_str),
                        ColorScheme::path(&target_str)
                    ),
                }
            }
        } else {
            let base = if source_str == target_str {
                format!("{op}: {source_str}")
            } else {
                format!("{op}: {source_str} → {target_str}")
            };

            match description {
                Some(desc) => println!("{base} ({desc})"),
                None => println!("{base}"),
            }
        }
    }

    /// Print symlink operation.
    pub fn link(
        &self,
        source: &std::path::Path,
        target: &std::path::Path,
        description: Option<&str>,
    ) {
        self.print_file_op("Linking", source, target, description);
    }

    /// Print copy operation.
    pub fn copy(
        &self,
        source: &std::path::Path,
        target: &std::path::Path,
        description: Option<&str>,
    ) {
        self.print_file_op("Copying", source, target, description);
    }

    /// Print a per-entry summary for multiple clean (no-conflict) link
    /// operations from a single glob-expanding config entry, instead of
    /// one line per file.
    pub fn link_summary(&self, count: usize, source_pattern: &str) {
        self.print_summary("Linked", count, source_pattern);
    }

    /// Print a per-entry summary for multiple clean (no-conflict) copy
    /// operations from a single glob-expanding config entry, instead of
    /// one line per file.
    pub fn copy_summary(&self, count: usize, source_pattern: &str) {
        self.print_summary("Copied", count, source_pattern);
    }

    fn print_summary(&self, op: &str, count: usize, source_pattern: &str) {
        if self.quiet {
            return;
        }
        if self.color.is_enabled() {
            println!(
                "{}: {} items ({})",
                ColorScheme::operation(op),
                count,
                ColorScheme::path(source_pattern)
            );
        } else {
            println!("{op}: {count} items ({source_pattern})");
        }
    }

    /// Print skip message.
    pub fn skip(&self, path: &std::path::Path) {
        if !self.quiet {
            if self.color.is_enabled() {
                println!(
                    "{}: {} (conflict)",
                    ColorScheme::skip("Skipped"),
                    ColorScheme::path(&path.display().to_string())
                );
            } else {
                println!("Skipped: {} (conflict)", path.display());
            }
        }
    }

    /// Print dry-run message.
    pub fn dry_run(&self, message: &str) {
        if !self.quiet {
            if self.color.is_enabled() {
                println!("{}", ColorScheme::dimmed(&format!("[dry-run] {message}")));
            } else {
                println!("[dry-run] {message}");
            }
        }
    }

    /// Print worktree removal message.
    pub fn remove(&self, path: &std::path::Path) {
        if !self.quiet {
            if self.color.is_enabled() {
                println!(
                    "{}: {}",
                    ColorScheme::operation("Removed"),
                    ColorScheme::path(&path.display().to_string())
                );
            } else {
                println!("Removed: {}", path.display());
            }
        }
    }

    /// Print safety warning.
    pub fn safety_warning(&self, path: &std::path::Path, message: &str) {
        if !self.quiet {
            if self.color.is_enabled() {
                println!(
                    "{}: {} - {}",
                    ColorScheme::warning("Warning"),
                    ColorScheme::path(&path.display().to_string()),
                    message
                );
            } else {
                println!("Warning: {} - {}", path.display(), message);
            }
        }
    }

    /// Print list item (suppressed in quiet mode).
    pub fn list(&self, line: &str) {
        if !self.quiet {
            println!("{line}");
        }
    }

    /// Print hook execution start message.
    pub fn hook_running(
        &self,
        hook_type: &str,
        index: usize,
        total: usize,
        command: &str,
        description: Option<&str>,
    ) {
        if !self.quiet {
            let display_text = description.unwrap_or(command);
            if self.color.is_enabled() {
                println!(
                    "{} {} hook {}: {}",
                    ColorScheme::hook_running("Running"),
                    ColorScheme::hook_type(hook_type),
                    ColorScheme::dimmed(&format!("[{}/{}]", index, total)),
                    ColorScheme::dimmed(display_text)
                );
            } else {
                println!(
                    "Running {} hook [{}/{}]: {}",
                    hook_type, index, total, display_text
                );
            }
        }
    }

    /// Print blank line after hook execution (separator).
    pub fn hook_separator(&self) {
        if !self.quiet {
            println!();
        }
    }

    /// Print hook failure warning.
    pub fn hook_warning(&self, hook_type: &str, error: &str, exit_code: Option<i32>) {
        if !self.quiet {
            eprintln!(); // Add blank line before warning
            if self.color.is_enabled() {
                eprintln!(
                    "{}: {} hook failed: {}",
                    ColorScheme::warning("Warning"),
                    hook_type,
                    error
                );
                if let Some(code) = exit_code {
                    eprintln!("  {}: {}", ColorScheme::exit_code("Exit code"), code);
                }
            } else {
                eprintln!("Warning: {} hook failed: {}", hook_type, error);
                if let Some(code) = exit_code {
                    eprintln!("  Exit code: {}", code);
                }
            }
        }
    }

    /// Print hook failure note.
    pub fn hook_note(&self, message: &str) {
        if !self.quiet {
            if self.color.is_enabled() {
                println!("{}", ColorScheme::dimmed(message));
            } else {
                println!("{}", message);
            }
        }
    }

    /// Print success result (all operations succeeded).
    pub fn results_success(&self, message: &str) {
        if !self.quiet {
            if self.color.is_enabled() {
                println!("{} {}", ColorScheme::success_label("[OK]"), message);
            } else {
                println!("[OK] {}", message);
            }
        }
    }

    /// Print results header.
    pub fn results_header(&self) {
        if !self.quiet {
            println!("Results:");
        }
    }

    /// Print a successful results item.
    pub fn results_item_success(&self, message: &str) {
        if !self.quiet {
            if self.color.is_enabled() {
                println!("  {} {}", ColorScheme::success_label("[OK]"), message);
            } else {
                println!("  [OK] {}", message);
            }
        }
    }

    /// Print a failed results item.
    pub fn results_item_failed(&self, message: &str) {
        if !self.quiet {
            if self.color.is_enabled() {
                println!("  {} {}", ColorScheme::failure_label("[FAIL]"), message);
            } else {
                println!("  [FAIL] {}", message);
            }
        }
    }

    /// Print failure details for a hook.
    pub fn results_failed_detail(
        &self,
        description: Option<&str>,
        command: &str,
        exit_code: Option<i32>,
    ) {
        if !self.quiet {
            let display = description.unwrap_or(command);
            println!("      - {}", display);

            if self.color.is_enabled() {
                println!("        {}: {}", ColorScheme::dimmed("Command"), command);
                if let Some(code) = exit_code {
                    println!("        {}: {}", ColorScheme::exit_code("Exit code"), code);
                }
            } else {
                println!("        Command: {}", command);
                if let Some(code) = exit_code {
                    println!("        Exit code: {}", code);
                }
            }
        }
    }
}
