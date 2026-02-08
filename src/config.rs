use crate::error::{Error, Result};

use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Config directory name
pub const CONFIG_DIR_NAME: &str = ".kabu";
/// Config file name for YAML format
pub const CONFIG_FILE_NAME_YAML: &str = "config.yaml";
/// Config file name for TOML format
pub const CONFIG_FILE_NAME_TOML: &str = "config.toml";
const GLOBAL_CONFIG_DIR_NAME: &str = "kabu";
/// Global config file name for YAML format
pub const GLOBAL_CONFIG_FILE_NAME_YAML: &str = "config.yaml";
/// Global config file name for TOML format
pub const GLOBAL_CONFIG_FILE_NAME_TOML: &str = "config.toml";

#[derive(Debug, Clone, Copy)]
enum ConfigFormat {
    Yaml,
    Toml,
}

/// Find config file with format detection. YAML takes priority.
fn find_config_file(
    dir: &Path,
    yaml_name: &str,
    toml_name: &str,
) -> Option<(PathBuf, ConfigFormat)> {
    let yaml_path = dir.join(yaml_name);
    if yaml_path.exists() {
        return Some((yaml_path, ConfigFormat::Yaml));
    }
    let toml_path = dir.join(toml_name);
    if toml_path.exists() {
        return Some((toml_path, ConfigFormat::Toml));
    }
    None
}

fn parse_raw_config(content: &str, format: ConfigFormat) -> std::result::Result<RawConfig, String> {
    match format {
        ConfigFormat::Yaml => serde_yaml::from_str(content).map_err(|e| e.to_string()),
        ConfigFormat::Toml => toml::from_str(content).map_err(|e| e.to_string()),
    }
}

fn global_config_dir() -> Option<PathBuf> {
    let base = env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(dirs::config_dir)
        .or_else(|| dirs::home_dir().map(|home| home.join(".config")))?;
    Some(base.join(GLOBAL_CONFIG_DIR_NAME))
}

/// Load config from the repository root. Returns None if config file doesn't exist.
pub(crate) fn load(repo_root: &Path) -> Result<Option<Config>> {
    let config_dir = repo_root.join(CONFIG_DIR_NAME);

    let (config_path, format) =
        match find_config_file(&config_dir, CONFIG_FILE_NAME_YAML, CONFIG_FILE_NAME_TOML) {
            Some(result) => result,
            None => return Ok(None),
        };

    let content = fs::read_to_string(&config_path)?;

    // Parse into RawConfig (permissive, all fields optional)
    let raw: RawConfig =
        parse_raw_config(&content, format).map_err(|message| Error::ConfigParse { message })?;

    // Convert to Config (validates and transforms)
    Config::try_from(raw).map(Some)
}

/// Load global config. Returns None if config file doesn't exist or config dir is unknown.
pub(crate) fn load_global() -> Result<Option<Config>> {
    let config_dir = match global_config_dir() {
        Some(dir) => dir,
        None => return Ok(None),
    };

    let (config_path, format) = match find_config_file(
        &config_dir,
        GLOBAL_CONFIG_FILE_NAME_YAML,
        GLOBAL_CONFIG_FILE_NAME_TOML,
    ) {
        Some(result) => result,
        None => return Ok(None),
    };

    let content = fs::read_to_string(&config_path)?;

    let raw: RawConfig = parse_raw_config(&content, format)
        .map_err(|message| Error::GlobalConfigParse { message })?;

    validate_global_config(&raw)?;

    let config = Config::try_from(raw).map_err(|err| match err {
        Error::ConfigValidation { message } => Error::GlobalConfigValidation { message },
        other => other,
    })?;

    Ok(Some(config))
}

/// Load config merged with global config. Repo config overrides global settings.
pub(crate) fn load_merged(repo_root: &Path) -> Result<Config> {
    let global = load_global()?;
    let repo = load(repo_root)?.unwrap_or_default();
    Ok(merge_with_global(repo, global.as_ref()))
}

fn validate_global_config(raw: &RawConfig) -> Result<()> {
    let mut errors = Vec::new();

    if raw.hooks.has_hooks() {
        errors.push("  - hooks are not allowed in global config".to_string());
    }
    if !raw.mkdir.is_empty() {
        errors.push("  - mkdir entries are not allowed in global config".to_string());
    }
    if !raw.link.is_empty() {
        errors.push("  - link entries are not allowed in global config".to_string());
    }
    if !raw.copy.is_empty() {
        errors.push("  - copy entries are not allowed in global config".to_string());
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(Error::GlobalConfigValidation {
            message: errors.join("\n"),
        })
    }
}

// Raw types for permissive YAML parsing. Missing fields get default values
// instead of parse errors, allowing validation to collect all errors at once.

#[derive(Debug, Deserialize, Default, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "kabu configuration",
    description = "Configuration file for kabu"
)]
pub(crate) struct RawConfig {
    on_conflict: Option<OnConflict>,
    #[serde(default)]
    auto_cd: RawAutoCd,
    #[serde(default)]
    worktree: RawWorktree,
    #[serde(default)]
    ui: RawUi,
    #[serde(default)]
    hooks: RawHooks,
    #[serde(default)]
    mkdir: Vec<RawMkdir>,
    #[serde(default)]
    link: Vec<RawLink>,
    #[serde(default)]
    copy: Vec<RawCopy>,
}

#[derive(Debug, Deserialize, Default, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    rename = "AutoCd",
    title = "Auto Cd",
    description = "Automatic directory change after worktree operations"
)]
struct RawAutoCd {
    after_add: Option<bool>,
    after_remove: Option<AfterRemove>,
}

#[derive(Debug, Deserialize, Default, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    rename = "Worktree",
    title = "Worktree",
    description = "Worktree path and branch template configuration with template variable support"
)]
struct RawWorktree {
    path_template: Option<String>,
    branch_template: Option<String>,
}

#[derive(Debug, Deserialize, Default, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    rename = "Ui",
    title = "UI",
    description = "Interactive UI configuration"
)]
struct RawUi {
    #[serde(default)]
    colors: RawUiColors,
    #[schemars(description = "Show key hints in the UI footer (default: true)")]
    show_key_hints: Option<bool>,
    #[schemars(
        description = "Default mode for interactive add: existing or new (default: existing)"
    )]
    add_default_mode: Option<AddDefaultMode>,
}

#[derive(Debug, Deserialize, Default, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    rename = "UiColors",
    title = "UI Colors",
    description = "Color overrides for interactive UI"
)]
struct RawUiColors {
    border: Option<String>,
    text: Option<String>,
    accent: Option<String>,
    header: Option<String>,
    footer: Option<String>,
    title: Option<String>,
    label: Option<String>,
    muted: Option<String>,
    disabled: Option<String>,
    search: Option<String>,
    preview: Option<String>,
    selection_bg: Option<String>,
    selection_fg: Option<String>,
    warning: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UiColor {
    Named(UiColorName),
    Rgb(u8, u8, u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UiColorName {
    Default,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    Gray,
    DarkGray,
    LightRed,
    LightGreen,
    LightYellow,
    LightBlue,
    LightMagenta,
    LightCyan,
    White,
}

/// Default mode for the interactive add command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema, Default)]
#[schemars(title = "Add Default Mode")]
#[serde(rename_all = "lowercase")]
pub(crate) enum AddDefaultMode {
    /// Cursor starts on "Use existing branch"
    Existing,
    /// Cursor starts on "Create new branch"
    #[default]
    New,
}

/// Hook entry with command and optional description.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "Hook Entry",
    description = "A hook command with optional description"
)]
pub(crate) struct HookEntry {
    pub command: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, Default, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    rename = "Hooks",
    title = "Hooks",
    description = "Hooks that run before/after worktree operations"
)]
struct RawHooks {
    hook_shell: Option<String>,
    #[serde(default)]
    pre_add: Vec<HookEntry>,
    #[serde(default)]
    post_add: Vec<HookEntry>,
    #[serde(default)]
    pre_remove: Vec<HookEntry>,
    #[serde(default)]
    post_remove: Vec<HookEntry>,
}

impl RawHooks {
    fn has_hooks(&self) -> bool {
        !self.pre_add.is_empty()
            || !self.post_add.is_empty()
            || !self.pre_remove.is_empty()
            || !self.post_remove.is_empty()
    }
}

#[derive(Debug, Deserialize, Default, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    rename = "MkdirEntry",
    title = "Mkdir Entry",
    description = "Directory creation operation"
)]
struct RawMkdir {
    #[serde(default)]
    path: PathBuf,
    description: Option<String>,
}

#[derive(Debug, Deserialize, Default, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    rename = "LinkEntry",
    title = "Link Entry",
    description = "Symlink creation operation with glob pattern support"
)]
struct RawLink {
    #[serde(default)]
    source: PathBuf,
    target: Option<PathBuf>,
    on_conflict: Option<OnConflict>,
    description: Option<String>,
    #[serde(default)]
    ignore_tracked: bool,
}

#[derive(Debug, Deserialize, Default, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    rename = "CopyEntry",
    title = "Copy Entry",
    description = "File/directory copy operation"
)]
struct RawCopy {
    #[serde(default)]
    source: PathBuf,
    target: Option<PathBuf>,
    on_conflict: Option<OnConflict>,
    description: Option<String>,
}

// Validated types used by the application. Guaranteed valid after TryFrom conversion.

/// Root configuration from .kabu/config.yaml.
#[derive(Debug, Default, Clone)]
pub(crate) struct Config {
    pub on_conflict: Option<OnConflict>,
    pub auto_cd: AutoCd,
    pub worktree: Worktree,
    pub ui: Ui,
    pub hooks: Hooks,
    pub mkdir: Vec<Mkdir>,
    pub link: Vec<Link>,
    pub copy: Vec<Copy>,
}

pub(crate) fn merge_with_global(mut repo: Config, global: Option<&Config>) -> Config {
    let Some(global) = global else {
        return repo;
    };

    if repo.on_conflict.is_none() {
        repo.on_conflict = global.on_conflict;
    }

    // auto_cd: use global values if repo values are not set
    if repo.auto_cd.after_add.is_none() {
        repo.auto_cd.after_add = global.auto_cd.after_add;
    }
    if repo.auto_cd.after_remove.is_none() {
        repo.auto_cd.after_remove = global.auto_cd.after_remove;
    }

    if repo.worktree.path_template.is_none() {
        repo.worktree.path_template = global.worktree.path_template.clone();
    }

    if repo.worktree.branch_template.is_none() {
        repo.worktree.branch_template = global.worktree.branch_template.clone();
    }

    repo.ui.colors = repo.ui.colors.merge_with_fallback(&global.ui.colors);
    if repo.ui.show_key_hints.is_none() {
        repo.ui.show_key_hints = global.ui.show_key_hints;
    }
    if repo.ui.add_default_mode.is_none() {
        repo.ui.add_default_mode = global.ui.add_default_mode;
    }

    if repo.hooks.hook_shell.is_none() {
        repo.hooks.hook_shell = global.hooks.hook_shell.clone();
    }

    repo
}

impl TryFrom<RawConfig> for Config {
    type Error = Error;

    fn try_from(raw: RawConfig) -> Result<Self> {
        let mut errors = Vec::new();
        let mut targets = HashSet::new();

        // No validation needed for worktree.path
        // It can contain variables or not, both are valid

        // Validate and convert mkdir entries
        let mut mkdir = Vec::with_capacity(raw.mkdir.len());
        for (i, raw_mkdir) in raw.mkdir.into_iter().enumerate() {
            let prefix = format!("mkdir[{i}]");

            if raw_mkdir.path.as_os_str().is_empty() {
                errors.push(format!("  - {prefix}: path is required"));
                continue;
            }

            if let Some(err) = validate_path(&raw_mkdir.path) {
                errors.push(format!("  - {prefix}.path: {err}"));
            }

            if !targets.insert(raw_mkdir.path.clone()) {
                errors.push(format!(
                    "  - {prefix}: duplicate target path: {}",
                    raw_mkdir.path.display()
                ));
            }

            mkdir.push(Mkdir {
                path: raw_mkdir.path,
                description: raw_mkdir.description,
            });
        }

        // Validate and convert link entries
        let mut link = Vec::with_capacity(raw.link.len());
        for (i, raw_link) in raw.link.into_iter().enumerate() {
            let prefix = format!("link[{i}]");

            if raw_link.source.as_os_str().is_empty() {
                errors.push(format!("  - {prefix}: source is required"));
                continue;
            }

            if let Some(err) = validate_path(&raw_link.source) {
                errors.push(format!("  - {prefix}.source: {err}"));
            }

            let target = raw_link
                .target
                .clone()
                .unwrap_or_else(|| raw_link.source.clone());

            if let Some(err) = validate_path(&target) {
                errors.push(format!("  - {prefix}.target: {err}"));
            }

            if !targets.insert(target.clone()) {
                errors.push(format!(
                    "  - {prefix}: duplicate target path: {}",
                    target.display()
                ));
            }

            link.push(Link {
                source: raw_link.source,
                target,
                on_conflict: raw_link.on_conflict,
                description: raw_link.description,
                ignore_tracked: raw_link.ignore_tracked,
            });
        }

        // Validate and convert copy entries
        let mut copy = Vec::with_capacity(raw.copy.len());
        for (i, raw_copy) in raw.copy.into_iter().enumerate() {
            let prefix = format!("copy[{i}]");

            if raw_copy.source.as_os_str().is_empty() {
                errors.push(format!("  - {prefix}: source is required"));
                continue;
            }

            if let Some(err) = validate_path(&raw_copy.source) {
                errors.push(format!("  - {prefix}.source: {err}"));
            }

            let target = raw_copy
                .target
                .clone()
                .unwrap_or_else(|| raw_copy.source.clone());

            if let Some(err) = validate_path(&target) {
                errors.push(format!("  - {prefix}.target: {err}"));
            }

            if !targets.insert(target.clone()) {
                errors.push(format!(
                    "  - {prefix}: duplicate target path: {}",
                    target.display()
                ));
            }

            copy.push(Copy {
                source: raw_copy.source,
                target,
                on_conflict: raw_copy.on_conflict,
                description: raw_copy.description,
            });
        }

        if !errors.is_empty() {
            return Err(Error::ConfigValidation {
                message: errors.join("\n"),
            });
        }

        let mut ui_colors = UiColors::default();
        let mut parse_ui_color = |field: &str, value: Option<String>| {
            if let Some(value) = value {
                match UiColor::parse(&value) {
                    Ok(color) => Some(color),
                    Err(err) => {
                        errors.push(format!("  - ui.colors.{field}: {err}"));
                        None
                    }
                }
            } else {
                None
            }
        };

        ui_colors.border = parse_ui_color("border", raw.ui.colors.border);
        ui_colors.text = parse_ui_color("text", raw.ui.colors.text);
        ui_colors.accent = parse_ui_color("accent", raw.ui.colors.accent);
        ui_colors.header = parse_ui_color("header", raw.ui.colors.header);
        ui_colors.footer = parse_ui_color("footer", raw.ui.colors.footer);
        ui_colors.title = parse_ui_color("title", raw.ui.colors.title);
        ui_colors.label = parse_ui_color("label", raw.ui.colors.label);
        ui_colors.muted = parse_ui_color("muted", raw.ui.colors.muted);
        ui_colors.disabled = parse_ui_color("disabled", raw.ui.colors.disabled);
        ui_colors.search = parse_ui_color("search", raw.ui.colors.search);
        ui_colors.preview = parse_ui_color("preview", raw.ui.colors.preview);
        ui_colors.selection_bg = parse_ui_color("selection_bg", raw.ui.colors.selection_bg);
        ui_colors.selection_fg = parse_ui_color("selection_fg", raw.ui.colors.selection_fg);
        ui_colors.warning = parse_ui_color("warning", raw.ui.colors.warning);
        ui_colors.error = parse_ui_color("error", raw.ui.colors.error);

        // Validate branch_template if present
        if let Some(ref branch_template) = raw.worktree.branch_template {
            let template_errors = validate_branch_template(branch_template);
            for error in template_errors {
                errors.push(format!("  - worktree.branch_template: {}", error));
            }
        }

        if !errors.is_empty() {
            return Err(Error::ConfigValidation {
                message: errors.join("\n"),
            });
        }

        Ok(Config {
            on_conflict: raw.on_conflict,
            auto_cd: AutoCd {
                after_add: raw.auto_cd.after_add,
                after_remove: raw.auto_cd.after_remove,
            },
            worktree: Worktree {
                path_template: raw.worktree.path_template,
                branch_template: raw.worktree.branch_template,
            },
            ui: Ui {
                colors: ui_colors,
                show_key_hints: raw.ui.show_key_hints,
                add_default_mode: raw.ui.add_default_mode,
            },
            hooks: Hooks {
                hook_shell: raw.hooks.hook_shell,
                pre_add: raw.hooks.pre_add,
                post_add: raw.hooks.post_add,
                pre_remove: raw.hooks.pre_remove,
                post_remove: raw.hooks.post_remove,
            },
            mkdir,
            link,
            copy,
        })
    }
}

/// Validate a path and return an error message if invalid.
fn validate_path(path: &Path) -> Option<String> {
    // Check for absolute paths (including Unix-style on Windows for consistent validation)
    let is_absolute = path.is_absolute()
        || path
            .to_str()
            .is_some_and(|s| s.starts_with('/') || s.starts_with('\\'));

    if is_absolute {
        return Some(format!(
            "absolute paths are not allowed: {}",
            path.display()
        ));
    }

    for component in path.components() {
        if component == std::path::Component::ParentDir {
            return Some(format!(
                "path traversal (..) is not allowed: {}",
                path.display()
            ));
        }
        if matches!(component, std::path::Component::Prefix(_)) {
            return Some(format!(
                "drive-relative paths are not allowed: {}",
                path.display()
            ));
        }
    }

    None
}

/// Automatic directory change configuration.
#[derive(Debug, Default, Clone)]
pub(crate) struct AutoCd {
    pub after_add: Option<bool>,
    pub after_remove: Option<AfterRemove>,
}

impl AutoCd {
    /// Get effective after_add value (default: true)
    pub fn after_add(&self) -> bool {
        self.after_add.unwrap_or(true)
    }

    /// Get effective after_remove value (default: main)
    pub fn after_remove(&self) -> AfterRemove {
        self.after_remove.unwrap_or(AfterRemove::Main)
    }
}

/// Worktree path generation configuration.
#[derive(Debug, Clone, Default)]
pub(crate) struct Worktree {
    pub path_template: Option<String>,
    pub branch_template: Option<String>,
}

/// Interactive UI configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub(crate) struct Ui {
    pub colors: UiColors,
    pub show_key_hints: Option<bool>,
    pub add_default_mode: Option<AddDefaultMode>,
}

impl Ui {
    /// Returns show_key_hints value, defaulting to true if not set.
    pub fn show_key_hints(&self) -> bool {
        self.show_key_hints.unwrap_or(true)
    }

    /// Returns add_default_mode value, defaulting to New if not set.
    pub fn add_default_mode(&self) -> AddDefaultMode {
        self.add_default_mode.unwrap_or_default()
    }
}

/// Customizable UI colors.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct UiColors {
    pub border: Option<UiColor>,
    pub text: Option<UiColor>,
    pub accent: Option<UiColor>,
    pub header: Option<UiColor>,
    pub footer: Option<UiColor>,
    pub title: Option<UiColor>,
    pub label: Option<UiColor>,
    pub muted: Option<UiColor>,
    pub disabled: Option<UiColor>,
    pub search: Option<UiColor>,
    pub preview: Option<UiColor>,
    pub selection_bg: Option<UiColor>,
    pub selection_fg: Option<UiColor>,
    pub warning: Option<UiColor>,
    pub error: Option<UiColor>,
}

impl UiColors {
    fn merge_with_fallback(&self, fallback: &UiColors) -> UiColors {
        UiColors {
            border: self.border.or(fallback.border),
            text: self.text.or(fallback.text),
            accent: self.accent.or(fallback.accent),
            header: self.header.or(fallback.header),
            footer: self.footer.or(fallback.footer),
            title: self.title.or(fallback.title),
            label: self.label.or(fallback.label),
            muted: self.muted.or(fallback.muted),
            disabled: self.disabled.or(fallback.disabled),
            search: self.search.or(fallback.search),
            preview: self.preview.or(fallback.preview),
            selection_bg: self.selection_bg.or(fallback.selection_bg),
            selection_fg: self.selection_fg.or(fallback.selection_fg),
            warning: self.warning.or(fallback.warning),
            error: self.error.or(fallback.error),
        }
    }
}

impl Worktree {
    /// Generate suggested worktree path based on configuration.
    /// Returns None if no worktree config is set.
    pub fn generate_path(&self, branch: &str, repository: &str) -> Option<String> {
        self.path_template.as_ref().map(|path_template| {
            let expanded = expand_variables(path_template, branch, repository);
            // If no variables were used, append branch at the end (backward compatibility)
            if expanded == *path_template {
                format!("{}{}", path_template, branch)
            } else {
                expanded
            }
        })
    }

    /// Generate suggested branch name based on branch_template configuration.
    /// Returns None if no branch_template is configured.
    pub fn generate_branch_name(&self, env: &BranchTemplateEnv) -> Option<String> {
        self.branch_template
            .as_ref()
            .map(|template| expand_branch_template(template, env))
    }
}

/// Template environment for branch name generation.
pub(crate) struct BranchTemplateEnv {
    pub commitish: String,
    pub repository: String,
}

impl UiColor {
    pub(crate) fn parse(value: &str) -> std::result::Result<Self, String> {
        let value = value.trim();
        if let Some(hex) = value.strip_prefix('#') {
            if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err("invalid hex color (expected #RRGGBB)".to_string());
            }
            let r =
                u8::from_str_radix(&hex[0..2], 16).map_err(|_| "invalid hex color".to_string())?;
            let g =
                u8::from_str_radix(&hex[2..4], 16).map_err(|_| "invalid hex color".to_string())?;
            let b =
                u8::from_str_radix(&hex[4..6], 16).map_err(|_| "invalid hex color".to_string())?;
            return Ok(UiColor::Rgb(r, g, b));
        }

        let normalized = value.to_ascii_lowercase();
        let name = match normalized.as_str() {
            "default" => UiColorName::Default,
            "black" => UiColorName::Black,
            "red" => UiColorName::Red,
            "green" => UiColorName::Green,
            "yellow" => UiColorName::Yellow,
            "blue" => UiColorName::Blue,
            "magenta" => UiColorName::Magenta,
            "cyan" => UiColorName::Cyan,
            "gray" => UiColorName::Gray,
            "dark-gray" => UiColorName::DarkGray,
            "light-red" => UiColorName::LightRed,
            "light-green" => UiColorName::LightGreen,
            "light-yellow" => UiColorName::LightYellow,
            "light-blue" => UiColorName::LightBlue,
            "light-magenta" => UiColorName::LightMagenta,
            "light-cyan" => UiColorName::LightCyan,
            "white" => UiColorName::White,
            _ => {
                return Err("unknown color name".to_string());
            }
        };
        Ok(UiColor::Named(name))
    }

    fn as_string(self) -> String {
        match self {
            UiColor::Rgb(r, g, b) => format!("#{:02x}{:02x}{:02x}", r, g, b),
            UiColor::Named(name) => name.as_str().to_string(),
        }
    }
}

impl UiColorName {
    fn as_str(self) -> &'static str {
        match self {
            UiColorName::Default => "default",
            UiColorName::Black => "black",
            UiColorName::Red => "red",
            UiColorName::Green => "green",
            UiColorName::Yellow => "yellow",
            UiColorName::Blue => "blue",
            UiColorName::Magenta => "magenta",
            UiColorName::Cyan => "cyan",
            UiColorName::Gray => "gray",
            UiColorName::DarkGray => "dark-gray",
            UiColorName::LightRed => "light-red",
            UiColorName::LightGreen => "light-green",
            UiColorName::LightYellow => "light-yellow",
            UiColorName::LightBlue => "light-blue",
            UiColorName::LightMagenta => "light-magenta",
            UiColorName::LightCyan => "light-cyan",
            UiColorName::White => "white",
        }
    }
}

impl Serialize for UiColor {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.as_string())
    }
}

impl<'de> Deserialize<'de> for UiColor {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        UiColor::parse(&value).map_err(serde::de::Error::custom)
    }
}

/// Expand variables in a string template.
/// Supports {{var}} syntax with optional whitespace.
/// Examples: {{branch}}, {{ branch }}, {{  branch  }}
fn expand_variables(template: &str, branch: &str, repository: &str) -> String {
    let mut result = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '{' {
            // Check if it's {{ (double brace)
            if chars.peek() == Some(&'{') {
                chars.next(); // consume second {

                // Collect variable name until closing }}
                let mut var_name = String::new();
                let mut found_close = false;

                while let Some(ch) = chars.next() {
                    if ch == '}' {
                        // For {{var}}, need to find second }
                        if chars.peek() == Some(&'}') {
                            chars.next(); // consume second }
                            found_close = true;
                            break;
                        } else {
                            var_name.push(ch);
                        }
                    } else {
                        var_name.push(ch);
                    }
                }

                if found_close {
                    // Trim whitespace and replace variable
                    let trimmed = var_name.trim();
                    match trimmed {
                        "branch" => result.push_str(branch),
                        "repository" => result.push_str(repository),
                        _ => {
                            // Unknown variable, keep original
                            result.push_str("{{");
                            result.push_str(&var_name);
                            result.push_str("}}");
                        }
                    }
                } else {
                    // Unclosed brace, keep original
                    result.push_str("{{");
                    result.push_str(&var_name);
                }
            } else {
                // Single {, treat as literal
                result.push('{');
            }
        } else {
            result.push(ch);
        }
    }

    result
}

/// Expand variables in a branch template string.
/// Supports:
/// - {{commitish}}: The commit-ish (branch name, tag, commit hash, etc.)
/// - {{repository}}: Repository name
/// - {{strftime(FORMAT)}}: Date formatting
/// - {{{literal}}}: Outputs literal {{literal}} (escape syntax)
fn expand_branch_template(template: &str, env: &BranchTemplateEnv) -> String {
    let mut result = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '{' {
            // Check if it's {{ or {{{ (double or triple brace)
            if chars.peek() == Some(&'{') {
                chars.next(); // consume second {

                // Check for triple brace (escape syntax)
                let is_escape = chars.peek() == Some(&'{');
                if is_escape {
                    chars.next(); // consume third {
                }

                let mut content = String::new();
                let mut found_close = false;

                while let Some(c) = chars.next() {
                    if c == '}' {
                        // For {{var}} or {{{var}}}, need to find matching closing braces
                        if chars.peek() == Some(&'}') {
                            chars.next(); // consume second }
                            if is_escape {
                                // For {{{var}}}, need third }
                                if chars.peek() == Some(&'}') {
                                    chars.next(); // consume third }
                                    found_close = true;
                                    break;
                                } else {
                                    // Only two }, treat as content
                                    content.push_str("}}");
                                }
                            } else {
                                found_close = true;
                                break;
                            }
                        } else {
                            content.push(c);
                        }
                    } else {
                        content.push(c);
                    }
                }

                if found_close {
                    let trimmed = content.trim();
                    if is_escape {
                        // Triple brace: output as literal {{content}}
                        result.push_str("{{");
                        result.push_str(trimmed);
                        result.push_str("}}");
                    } else {
                        // Double brace: expand as variable
                        let expanded = expand_branch_variable(trimmed, env);
                        result.push_str(&expanded);
                    }
                } else {
                    // Unclosed braces, keep original
                    if is_escape {
                        result.push_str("{{{");
                    } else {
                        result.push_str("{{");
                    }
                    result.push_str(&content);
                }
            } else {
                // Single {, treat as literal
                result.push('{');
            }
        } else {
            result.push(ch);
        }
    }

    result
}

fn expand_branch_variable(var: &str, env: &BranchTemplateEnv) -> String {
    match var {
        "commitish" => env.commitish.clone(),
        "repository" => env.repository.clone(),
        _ if var.starts_with("strftime(") && var.ends_with(')') => {
            let format_str = &var[9..var.len() - 1];
            // Try to format with chrono, fallback to literal if format is invalid
            let formatted = chrono::Local::now().format(format_str);
            let mut result = String::new();
            match std::fmt::write(&mut result, format_args!("{}", formatted)) {
                Ok(_) => result,
                Err(_) => {
                    // Invalid format string, return as literal
                    format!("{{{}}}", var)
                }
            }
        }
        _ => {
            format!("{{{}}}", var)
        }
    }
}

/// Extract all template variables from a branch template.
/// Returns a Vec of variable names found (e.g., ["commitish", "strftime(...)"])
/// Triple braces ({{{...}}}) are escape syntax and not included in the result.
fn extract_template_variables(template: &str) -> Vec<String> {
    let mut variables = Vec::new();
    let mut chars = template.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '{' && chars.peek() == Some(&'{') {
            chars.next(); // consume second {

            // Check for triple brace (escape syntax)
            let is_escape = chars.peek() == Some(&'{');
            if is_escape {
                chars.next(); // consume third {
            }

            let mut content = String::new();
            let mut found_close = false;

            while let Some(c) = chars.next() {
                if c == '}' {
                    if chars.peek() == Some(&'}') {
                        chars.next(); // consume second }
                        if is_escape {
                            // For {{{var}}}, need third }
                            if chars.peek() == Some(&'}') {
                                chars.next(); // consume third }
                                found_close = true;
                                break;
                            } else {
                                content.push_str("}}");
                            }
                        } else {
                            found_close = true;
                            break;
                        }
                    } else {
                        content.push(c);
                    }
                } else {
                    content.push(c);
                }
            }

            // Only add to variables if it's a double brace (not escape syntax)
            if found_close && !is_escape {
                variables.push(content.trim().to_string());
            }
        }
    }

    variables
}

/// Check if a template variable is valid for branch_template expansion.
/// Valid variables: commitish, repository, strftime(...)
fn is_valid_branch_template_variable(var: &str) -> bool {
    match var {
        "commitish" | "repository" => true,
        _ if var.starts_with("strftime(") && var.ends_with(')') => true,
        _ => false,
    }
}

/// Validate branch_template and return error messages for invalid variables.
fn validate_branch_template(template: &str) -> Vec<String> {
    let variables = extract_template_variables(template);
    variables
        .iter()
        .filter(|var| !is_valid_branch_template_variable(var))
        .map(|var| {
            format!(
                "Invalid template variable '{{{}}}' in branch_template. Valid variables: {{{{commitish}}}}, {{{{repository}}}}, {{{{strftime(...)}}}}",
                var
            )
        })
        .collect()
}

/// Hook commands configuration.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct Hooks {
    pub hook_shell: Option<String>,
    pub pre_add: Vec<HookEntry>,
    pub post_add: Vec<HookEntry>,
    pub pre_remove: Vec<HookEntry>,
    pub post_remove: Vec<HookEntry>,
}

impl Hooks {
    /// Check if any hooks are defined.
    pub fn has_hooks(&self) -> bool {
        !self.pre_add.is_empty()
            || !self.post_add.is_empty()
            || !self.pre_remove.is_empty()
            || !self.post_remove.is_empty()
    }
}

/// Directory creation configuration entry.
#[derive(Debug, Clone)]
pub(crate) struct Mkdir {
    pub path: PathBuf,
    pub description: Option<String>,
}

/// Symlink configuration entry.
#[derive(Debug, Clone)]
pub(crate) struct Link {
    pub source: PathBuf,
    pub target: PathBuf, // Always resolved (no Option)
    pub on_conflict: Option<OnConflict>,
    pub description: Option<String>,
    pub ignore_tracked: bool,
}

/// File copy configuration entry.
#[derive(Debug, Clone)]
pub(crate) struct Copy {
    pub source: PathBuf,
    pub target: PathBuf, // Always resolved (no Option)
    pub on_conflict: Option<OnConflict>,
    pub description: Option<String>,
}

/// Conflict resolution mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[schemars(
    title = "Conflict Resolution Mode",
    description = "How to handle file conflicts during operations"
)]
#[serde(rename_all = "lowercase")]
pub(crate) enum OnConflict {
    Abort,
    Skip,
    Overwrite,
    Backup,
}

/// Behavior after removing a worktree when the current directory is removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[schemars(
    title = "After Remove",
    description = "Where to cd after removing the current worktree"
)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AfterRemove {
    /// cd to the main worktree automatically
    Main,
    /// Show interactive selection to choose a worktree
    Select,
}

/// Snapshot of configuration for trust verification.
///
/// Captures all configuration settings at the time of trust to detect when
/// configuration changes require re-trust. This enables full configuration
/// tracking (not just hooks) for comprehensive security coverage.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct ConfigSnapshot {
    pub on_conflict: Option<OnConflict>,
    pub worktree: WorktreeSnapshot,
    pub hooks: Hooks,
    pub mkdir: Vec<MkdirSnapshot>,
    pub link: Vec<LinkSnapshot>,
    pub copy: Vec<CopySnapshot>,
}

/// Worktree configuration snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct WorktreeSnapshot {
    pub path_template: Option<String>,
    pub branch_template: Option<String>,
}

/// Mkdir operation snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct MkdirSnapshot {
    pub path: String,
    pub description: Option<String>,
}

/// Link operation snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct LinkSnapshot {
    pub source: String,
    pub target: String,
    pub on_conflict: Option<OnConflict>,
    pub description: Option<String>,
    pub ignore_tracked: bool,
}

/// Copy operation snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct CopySnapshot {
    pub source: String,
    pub target: String,
    pub on_conflict: Option<OnConflict>,
    pub description: Option<String>,
}

impl ConfigSnapshot {
    /// Convert a Config into a ConfigSnapshot.
    pub(crate) fn from_config(config: &Config) -> Self {
        ConfigSnapshot {
            on_conflict: config.on_conflict,
            worktree: WorktreeSnapshot {
                path_template: config.worktree.path_template.clone(),
                branch_template: config.worktree.branch_template.clone(),
            },
            hooks: config.hooks.clone(),
            mkdir: config
                .mkdir
                .iter()
                .map(|m| MkdirSnapshot {
                    path: m.path.to_string_lossy().to_string(),
                    description: m.description.clone(),
                })
                .collect(),
            link: config
                .link
                .iter()
                .map(|l| LinkSnapshot {
                    source: l.source.to_string_lossy().to_string(),
                    target: l.target.to_string_lossy().to_string(),
                    on_conflict: l.on_conflict,
                    description: l.description.clone(),
                    ignore_tracked: l.ignore_tracked,
                })
                .collect(),
            copy: config
                .copy
                .iter()
                .map(|c| CopySnapshot {
                    source: c.source.to_string_lossy().to_string(),
                    target: c.target.to_string_lossy().to_string(),
                    on_conflict: c.on_conflict,
                    description: c.description.clone(),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_config() {
        let yaml = r#"
link:
  - source: ".env.local"
        "#;

        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let config = Config::try_from(raw).unwrap();
        assert_eq!(config.link.len(), 1);
        assert_eq!(config.link[0].source, PathBuf::from(".env.local"));
        assert_eq!(config.link[0].target, PathBuf::from(".env.local"));
    }

    #[test]
    fn test_parse_full_config() {
        let yaml = r#"
on_conflict: skip

mkdir:
  - path: "tmp/cache"
    description: "Create cache dir"

link:
  - source: ".env.local"
  - source: ".secret/creds.json"
    target: "config/creds.json"
    on_conflict: abort
    description: "Link credentials"

copy:
  - source: ".env.example"
    target: ".env"
    on_conflict: backup
        "#;

        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let config = Config::try_from(raw).unwrap();

        assert_eq!(config.on_conflict, Some(OnConflict::Skip));

        assert_eq!(config.mkdir.len(), 1);
        assert_eq!(config.mkdir[0].path, PathBuf::from("tmp/cache"));
        assert_eq!(
            config.mkdir[0].description,
            Some("Create cache dir".to_string())
        );

        assert_eq!(config.link.len(), 2);
        assert_eq!(config.link[1].on_conflict, Some(OnConflict::Abort));
        assert_eq!(
            config.link[1].description,
            Some("Link credentials".to_string())
        );

        assert_eq!(config.copy.len(), 1);
        assert_eq!(config.copy[0].on_conflict, Some(OnConflict::Backup));
    }

    #[test]
    fn test_parse_empty_config() {
        let yaml = "";
        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let config = Config::try_from(raw).unwrap();
        assert!(config.link.is_empty());
        assert!(config.copy.is_empty());
        assert!(config.mkdir.is_empty());
    }

    #[test]
    fn test_parse_invalid_yaml() {
        let yaml = "invalid: yaml: [[[";
        let result: std::result::Result<RawConfig, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_missing_source() {
        let yaml = r#"
link:
  - target: ".env"
        "#;

        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let err = Config::try_from(raw).unwrap_err();
        assert!(err.to_string().contains("source is required"));
    }

    #[test]
    fn test_validate_missing_mkdir_path() {
        let yaml = r#"
mkdir:
  - description: "test"
        "#;

        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let err = Config::try_from(raw).unwrap_err();
        assert!(err.to_string().contains("path is required"));
    }

    #[test]
    fn test_validate_absolute_path() {
        let yaml = r#"
link:
  - source: "/etc/passwd"
        "#;

        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let err = Config::try_from(raw).unwrap_err();
        assert!(err.to_string().contains("absolute paths are not allowed"));
    }

    #[test]
    fn test_validate_path_traversal() {
        let yaml = r#"
copy:
  - source: "../../../etc/passwd"
    target: "passwd"
        "#;

        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let err = Config::try_from(raw).unwrap_err();
        assert!(err.to_string().contains("path traversal"));
    }

    #[test]
    fn test_validate_duplicate_targets() {
        let yaml = r#"
link:
  - source: ".env.local"
    target: ".env"
  - source: ".env.prod"
    target: ".env"
        "#;

        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let err = Config::try_from(raw).unwrap_err();
        assert!(err.to_string().contains("duplicate target path"));
    }

    #[test]
    fn test_validate_collects_multiple_errors() {
        let yaml = r#"
mkdir:
  - description: "no path"

link:
  - source: "/etc/passwd"

copy:
  - source: "../secret"
    target: "secret"
        "#;

        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let err = Config::try_from(raw).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("path is required"));
        assert!(msg.contains("absolute paths are not allowed"));
        assert!(msg.contains("path traversal"));
    }

    #[test]
    fn test_validate_multiple_missing_sources() {
        let yaml = r#"
copy:
  - description: "copy test1"
    target: "test1-copy"
  - description: "copy test2"
    target: "test2-copy"
        "#;

        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let err = Config::try_from(raw).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("copy[0]: source is required"));
        assert!(msg.contains("copy[1]: source is required"));
    }

    #[test]
    fn test_deny_unknown_fields_top_level() {
        let yaml = r#"
invalid_key: "some value"
link:
  - source: ".env"
        "#;

        let result: std::result::Result<RawConfig, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("unknown field"));
    }

    #[test]
    fn test_deny_unknown_fields_in_defaults() {
        let yaml = r#"
defaults:
  invalid_option: true
  on_conflict: skip
        "#;

        let result: std::result::Result<RawConfig, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("unknown field"));
    }

    #[test]
    fn test_deny_unknown_fields_in_link() {
        let yaml = r#"
link:
  - source: ".env"
    invalid_field: "test"
        "#;

        let result: std::result::Result<RawConfig, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("unknown field"));
    }

    #[test]
    fn test_deny_unknown_fields_in_copy() {
        let yaml = r#"
copy:
  - source: ".env"
    unknown_key: 123
        "#;

        let result: std::result::Result<RawConfig, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("unknown field"));
    }

    #[test]
    fn test_deny_unknown_fields_in_mkdir() {
        let yaml = r#"
mkdir:
  - path: "tmp"
    invalid: "value"
        "#;

        let result: std::result::Result<RawConfig, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("unknown field"));
    }

    #[test]
    fn test_deny_unknown_fields_in_hooks() {
        let yaml = r#"
hooks:
  invalid_hook: "test"
  pre_add:
    - command: "echo test"
        "#;

        let result: std::result::Result<RawConfig, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("unknown field"));
    }

    #[test]
    fn test_deny_unknown_fields_in_hook_entry() {
        let yaml = r#"
hooks:
  pre_add:
    - command: "echo test"
      invalid_field: "value"
        "#;

        let result: std::result::Result<RawConfig, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("unknown field"));
    }

    #[test]
    fn test_deny_unknown_fields_in_worktree() {
        let yaml = r#"
worktree:
  path: "../worktrees/"
  invalid_setting: true
        "#;

        let result: std::result::Result<RawConfig, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("unknown field"));
    }

    #[test]
    fn test_hooks_has_hooks_empty() {
        let hooks = Hooks::default();
        assert!(!hooks.has_hooks());
    }

    #[test]
    fn test_hooks_has_hooks_with_pre_add() {
        let hooks = Hooks {
            pre_add: vec![HookEntry {
                command: "echo test".to_string(),
                description: None,
            }],
            ..Default::default()
        };
        assert!(hooks.has_hooks());
    }

    #[test]
    fn test_hooks_has_hooks_with_post_add() {
        let hooks = Hooks {
            post_add: vec![HookEntry {
                command: "npm install".to_string(),
                description: None,
            }],
            ..Default::default()
        };
        assert!(hooks.has_hooks());
    }

    #[test]
    fn test_hooks_has_hooks_with_pre_remove() {
        let hooks = Hooks {
            pre_remove: vec![HookEntry {
                command: "echo cleanup".to_string(),
                description: None,
            }],
            ..Default::default()
        };
        assert!(hooks.has_hooks());
    }

    #[test]
    fn test_hooks_has_hooks_with_post_remove() {
        let hooks = Hooks {
            post_remove: vec![HookEntry {
                command: "./scripts/cleanup.sh".to_string(),
                description: None,
            }],
            ..Default::default()
        };
        assert!(hooks.has_hooks());
    }

    #[test]
    fn test_hooks_has_hooks_ignores_hook_shell_only() {
        let hooks = Hooks {
            hook_shell: Some("pwsh".to_string()),
            ..Default::default()
        };
        assert!(!hooks.has_hooks());
    }

    #[test]
    fn test_parse_config_with_hooks() {
        let yaml = r#"
hooks:
  pre_add:
    - command: "echo 'pre add'"
  post_add:
    - command: "npm install"
    - command: "mise install"
      description: "Install mise tools"
  pre_remove:
    - command: "echo 'pre remove'"
  post_remove:
    - command: "./scripts/cleanup.sh"
        "#;

        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let config = Config::try_from(raw).unwrap();

        assert_eq!(config.hooks.pre_add.len(), 1);
        assert_eq!(config.hooks.pre_add[0].command, "echo 'pre add'");
        assert_eq!(config.hooks.pre_add[0].description, None);

        assert_eq!(config.hooks.post_add.len(), 2);
        assert_eq!(config.hooks.post_add[0].command, "npm install");
        assert_eq!(config.hooks.post_add[0].description, None);
        assert_eq!(config.hooks.post_add[1].command, "mise install");
        assert_eq!(
            config.hooks.post_add[1].description,
            Some("Install mise tools".to_string())
        );

        assert_eq!(config.hooks.pre_remove.len(), 1);
        assert_eq!(config.hooks.pre_remove[0].command, "echo 'pre remove'");

        assert_eq!(config.hooks.post_remove.len(), 1);
        assert_eq!(config.hooks.post_remove[0].command, "./scripts/cleanup.sh");
    }

    #[test]
    fn test_parse_config_with_empty_hooks() {
        let yaml = r#"
hooks: {}
        "#;

        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let config = Config::try_from(raw).unwrap();

        assert!(!config.hooks.has_hooks());
        assert!(config.hooks.pre_add.is_empty());
        assert!(config.hooks.post_add.is_empty());
        assert!(config.hooks.pre_remove.is_empty());
        assert!(config.hooks.post_remove.is_empty());
    }

    #[test]
    fn test_parse_config_without_hooks() {
        let yaml = r#"
mkdir:
  - path: "build"
        "#;

        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let config = Config::try_from(raw).unwrap();

        assert!(!config.hooks.has_hooks());
    }

    #[test]
    fn test_parse_worktree_path() {
        let yaml = r#"
worktree:
  path_template: "../worktrees/"
        "#;
        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let config = Config::try_from(raw).unwrap();
        assert_eq!(
            config.worktree.path_template,
            Some("../worktrees/".to_string())
        );
    }

    #[test]
    fn test_parse_worktree_path_with_variables() {
        let yaml = r#"
worktree:
  path_template: "../{repository}-{branch}"
        "#;
        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let config = Config::try_from(raw).unwrap();
        assert_eq!(
            config.worktree.path_template,
            Some("../{repository}-{branch}".to_string())
        );
    }

    #[test]
    fn test_parse_worktree_empty() {
        let yaml = r#"
worktree: {}
        "#;
        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let config = Config::try_from(raw).unwrap();
        assert!(config.worktree.path_template.is_none());
    }

    #[test]
    fn test_parse_config_without_worktree() {
        let yaml = r#"
mkdir:
  - path: "build"
        "#;
        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let config = Config::try_from(raw).unwrap();
        assert!(config.worktree.path_template.is_none());
    }

    #[test]
    fn test_worktree_allows_absolute_path() {
        let yaml = r#"
worktree:
  path_template: "/home/user/worktrees/"
        "#;
        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let config = Config::try_from(raw).unwrap();
        assert_eq!(
            config.worktree.path_template,
            Some("/home/user/worktrees/".to_string())
        );
    }

    #[test]
    fn test_generate_path_without_variables() {
        let worktree = Worktree {
            path_template: Some("../worktrees/".to_string()),
            branch_template: None,
        };
        let result = worktree.generate_path("feature/foo", "myrepo");
        assert_eq!(result, Some("../worktrees/feature/foo".to_string()));
    }

    #[test]
    fn test_generate_path_with_variables() {
        let worktree = Worktree {
            path_template: Some("../{{repository}}-{{branch}}".to_string()),
            branch_template: None,
        };
        let result = worktree.generate_path("feature/foo", "myrepo");
        assert_eq!(result, Some("../myrepo-feature/foo".to_string()));
    }

    #[test]
    fn test_generate_path_with_branch_only() {
        let worktree = Worktree {
            path_template: Some("../wt-{{branch}}".to_string()),
            branch_template: None,
        };
        let result = worktree.generate_path("main", "myrepo");
        assert_eq!(result, Some("../wt-main".to_string()));
    }

    #[test]
    fn test_generate_path_with_repository_only() {
        let worktree = Worktree {
            path_template: Some("../{{repository}}-worktree".to_string()),
            branch_template: None,
        };
        let result = worktree.generate_path("feature/foo", "myrepo");
        assert_eq!(result, Some("../myrepo-worktree".to_string()));
    }

    #[test]
    fn test_generate_path_no_config() {
        let worktree = Worktree {
            path_template: None,
            branch_template: None,
        };
        let result = worktree.generate_path("feature/foo", "myrepo");
        assert_eq!(result, None);
    }

    #[test]
    fn test_generate_path_branch_with_slashes() {
        let worktree = Worktree {
            path_template: Some("../".to_string()),
            branch_template: None,
        };
        let result = worktree.generate_path("feature/deep/nested", "myrepo");
        assert_eq!(result, Some("../feature/deep/nested".to_string()));
    }

    #[test]
    fn test_generate_path_double_braces() {
        let worktree = Worktree {
            path_template: Some("../{{repository}}-{{branch}}-".to_string()),
            branch_template: None,
        };
        let result = worktree.generate_path("test", "myrepo");
        assert_eq!(result, Some("../myrepo-test-".to_string()));
    }

    #[test]
    fn test_parse_worktree_path_double_braces() {
        let yaml = r#"
worktree:
  path_template: "../{{repository}}-{{branch}}"
        "#;
        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let config = Config::try_from(raw).unwrap();
        assert_eq!(
            config.worktree.path_template,
            Some("../{{repository}}-{{branch}}".to_string())
        );
    }

    #[test]
    fn test_generate_path_with_whitespace_in_variables() {
        let worktree = Worktree {
            path_template: Some("../{{ branch }}-{{ repository }}".to_string()),
            branch_template: None,
        };
        let result = worktree.generate_path("test", "myrepo");
        assert_eq!(result, Some("../test-myrepo".to_string()));
    }

    #[test]
    fn test_generate_path_with_multiple_spaces() {
        let worktree = Worktree {
            path_template: Some("../{{  branch  }}-{{   repository   }}-".to_string()),
            branch_template: None,
        };
        let result = worktree.generate_path("foo", "bar");
        assert_eq!(result, Some("../foo-bar-".to_string()));
    }

    #[test]
    fn test_generate_path_single_braces_literal() {
        let worktree = Worktree {
            path_template: Some("../{branch}/{{ repository }}".to_string()),
            branch_template: None,
        };
        let result = worktree.generate_path("feature", "myrepo");
        // Single braces should be treated as literal
        assert_eq!(result, Some("../{branch}/myrepo".to_string()));
    }

    #[test]
    fn test_parse_worktree_path_with_spaces() {
        let yaml = r#"
worktree:
  path_template: "../{{ branch }}/{{ repository }}"
        "#;
        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let config = Config::try_from(raw).unwrap();
        assert!(config.worktree.path_template.is_some());
    }

    #[test]
    fn test_expand_branch_template_commitish() {
        let env = BranchTemplateEnv {
            commitish: "feature/auth".to_string(),
            repository: "myrepo".to_string(),
        };
        let result = expand_branch_template("review/{{commitish}}", &env);
        assert_eq!(result, "review/feature/auth");
    }

    #[test]
    fn test_expand_branch_template_repository() {
        let env = BranchTemplateEnv {
            commitish: "main".to_string(),
            repository: "myrepo".to_string(),
        };
        let result = expand_branch_template("{{repository}}/review/{{commitish}}", &env);
        assert_eq!(result, "myrepo/review/main");
    }

    #[test]
    fn test_expand_branch_template_strftime() {
        let env = BranchTemplateEnv {
            commitish: "fix".to_string(),
            repository: "myrepo".to_string(),
        };
        let result = expand_branch_template("hotfix/{{strftime(%Y)}}/{{commitish}}", &env);
        assert!(result.starts_with("hotfix/20"));
        assert!(result.ends_with("/fix"));
    }

    #[test]
    fn test_expand_branch_template_single_braces_literal() {
        let env = BranchTemplateEnv {
            commitish: "feature".to_string(),
            repository: "myrepo".to_string(),
        };
        // Single braces should be treated as literal
        let result = expand_branch_template("review/{commitish}", &env);
        assert_eq!(result, "review/{commitish}");
    }

    #[test]
    fn test_expand_branch_template_with_spaces() {
        let env = BranchTemplateEnv {
            commitish: "feature".to_string(),
            repository: "myrepo".to_string(),
        };
        let result = expand_branch_template("review/{{ commitish }}", &env);
        assert_eq!(result, "review/feature");
    }

    #[test]
    fn test_expand_branch_template_unknown_variable() {
        let env = BranchTemplateEnv {
            commitish: "main".to_string(),
            repository: "myrepo".to_string(),
        };
        let result = expand_branch_template("{{unknown}}/{{commitish}}", &env);
        assert_eq!(result, "{unknown}/main");
    }

    #[test]
    fn test_generate_branch_name() {
        let worktree = Worktree {
            path_template: None,
            branch_template: Some("review/{{commitish}}".to_string()),
        };
        let env = BranchTemplateEnv {
            commitish: "feature/auth".to_string(),
            repository: "myrepo".to_string(),
        };
        let result = worktree.generate_branch_name(&env);
        assert_eq!(result, Some("review/feature/auth".to_string()));
    }

    #[test]
    fn test_generate_branch_name_none() {
        let worktree = Worktree {
            path_template: None,
            branch_template: None,
        };
        let env = BranchTemplateEnv {
            commitish: "main".to_string(),
            repository: "myrepo".to_string(),
        };
        let result = worktree.generate_branch_name(&env);
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_worktree_branch_template() {
        let yaml = r#"
worktree:
  path_template: ../worktrees/{{branch}}
  branch_template: review/{{commitish}}
        "#;
        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let config = Config::try_from(raw).unwrap();
        assert_eq!(
            config.worktree.branch_template,
            Some("review/{{commitish}}".to_string())
        );
    }

    #[test]
    fn test_expand_branch_template_invalid_strftime() {
        let env = BranchTemplateEnv {
            commitish: "main".to_string(),
            repository: "myrepo".to_string(),
        };
        // Invalid format specifiers should not panic, return as literal
        let result = expand_branch_template("feat/{{strftime(%あ)}}", &env);
        assert_eq!(result, "feat/{strftime(%あ)}");

        // Mixed valid and invalid
        let result = expand_branch_template("{{commitish}}-{{strftime(%Y%m%d-%H%M%あ)}}", &env);
        assert!(result.starts_with("main-"));
        // The invalid part should be returned as literal
        assert!(result.contains("strftime"));
    }

    #[test]
    fn test_ui_color_parse_named_colors() {
        assert_eq!(
            UiColor::parse("red").unwrap(),
            UiColor::Named(UiColorName::Red)
        );
        assert_eq!(
            UiColor::parse("blue").unwrap(),
            UiColor::Named(UiColorName::Blue)
        );
        assert_eq!(
            UiColor::parse("dark-gray").unwrap(),
            UiColor::Named(UiColorName::DarkGray)
        );
        assert_eq!(
            UiColor::parse("light-cyan").unwrap(),
            UiColor::Named(UiColorName::LightCyan)
        );
        assert_eq!(
            UiColor::parse("default").unwrap(),
            UiColor::Named(UiColorName::Default)
        );
    }

    #[test]
    fn test_ui_color_parse_case_insensitive() {
        assert_eq!(
            UiColor::parse("RED").unwrap(),
            UiColor::Named(UiColorName::Red)
        );
        assert_eq!(
            UiColor::parse("Dark-Gray").unwrap(),
            UiColor::Named(UiColorName::DarkGray)
        );
        assert_eq!(
            UiColor::parse("LIGHT-BLUE").unwrap(),
            UiColor::Named(UiColorName::LightBlue)
        );
    }

    #[test]
    fn test_ui_color_parse_rgb_hex() {
        assert_eq!(UiColor::parse("#ff0000").unwrap(), UiColor::Rgb(255, 0, 0));
        assert_eq!(UiColor::parse("#00ff00").unwrap(), UiColor::Rgb(0, 255, 0));
        assert_eq!(UiColor::parse("#0000ff").unwrap(), UiColor::Rgb(0, 0, 255));
        assert_eq!(
            UiColor::parse("#123abc").unwrap(),
            UiColor::Rgb(0x12, 0x3a, 0xbc)
        );
        // Case insensitive hex
        assert_eq!(
            UiColor::parse("#AABBCC").unwrap(),
            UiColor::Rgb(170, 187, 204)
        );
    }

    #[test]
    fn test_ui_color_parse_whitespace_trimmed() {
        assert_eq!(
            UiColor::parse("  red  ").unwrap(),
            UiColor::Named(UiColorName::Red)
        );
        assert_eq!(
            UiColor::parse("  #ff0000  ").unwrap(),
            UiColor::Rgb(255, 0, 0)
        );
    }

    #[test]
    fn test_ui_color_parse_invalid() {
        assert!(UiColor::parse("invalid-color").is_err());
        assert!(UiColor::parse("#ff").is_err()); // too short
        assert!(UiColor::parse("#ff00ff00").is_err()); // too long
        assert!(UiColor::parse("#gggggg").is_err()); // invalid hex
        assert!(UiColor::parse("").is_err());
    }

    #[test]
    fn test_ui_colors_config_parsing() {
        let yaml = r##"
ui:
  colors:
    border: dark-gray
    accent: "#ff5500"
    selection_bg: blue
"##;
        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let config = Config::try_from(raw).unwrap();

        assert_eq!(
            config.ui.colors.border,
            Some(UiColor::Named(UiColorName::DarkGray))
        );
        assert_eq!(config.ui.colors.accent, Some(UiColor::Rgb(255, 85, 0)));
        assert_eq!(
            config.ui.colors.selection_bg,
            Some(UiColor::Named(UiColorName::Blue))
        );
        assert!(config.ui.colors.text.is_none());
    }

    #[test]
    fn test_ui_colors_invalid_color_error() {
        let yaml = r#"
ui:
  colors:
    border: invalid-color
"#;
        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let result = Config::try_from(raw);
        assert!(result.is_err());
    }

    // branch_template validation tests
    #[test]
    fn test_branch_template_valid_variables() {
        let yaml = r#"
worktree:
  branch_template: "feature/{{commitish}}-{{repository}}"
"#;
        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let result = Config::try_from(raw);
        assert!(result.is_ok());
    }

    #[test]
    fn test_branch_template_valid_strftime() {
        let yaml = r#"
worktree:
  branch_template: "{{commitish}}-{{strftime(%Y%m%d)}}"
"#;
        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let result = Config::try_from(raw);
        assert!(result.is_ok());
    }

    #[test]
    fn test_branch_template_invalid_variable() {
        let yaml = r#"
worktree:
  branch_template: "feature/{{branch}}-{{commitish}}"
"#;
        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let result = Config::try_from(raw);
        assert!(result.is_err());
        let error = result.unwrap_err().to_string();
        assert!(error.contains("branch_template"));
        assert!(error.contains("Invalid template variable"));
    }

    #[test]
    fn test_branch_template_multiple_invalid_variables() {
        let yaml = r#"
worktree:
  branch_template: "{{branch}}/{{invalidvar}}"
"#;
        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let result = Config::try_from(raw);
        assert!(result.is_err());
        let error = result.unwrap_err().to_string();
        assert!(error.contains("branch"));
        assert!(error.contains("invalidvar"));
    }

    #[test]
    fn test_extract_template_variables() {
        let vars = extract_template_variables("prefix-{{commitish}}-{{repository}}");
        assert_eq!(vars.len(), 2);
        assert!(vars.contains(&"commitish".to_string()));
        assert!(vars.contains(&"repository".to_string()));
    }

    #[test]
    fn test_extract_template_variables_with_strftime() {
        let vars = extract_template_variables("{{strftime(%Y-%m-%d)}}-{{commitish}}");
        assert_eq!(vars.len(), 2);
        assert!(vars.contains(&"strftime(%Y-%m-%d)".to_string()));
        assert!(vars.contains(&"commitish".to_string()));
    }

    #[test]
    fn test_is_valid_branch_template_variable() {
        assert!(is_valid_branch_template_variable("commitish"));
        assert!(is_valid_branch_template_variable("repository"));
        assert!(is_valid_branch_template_variable("strftime(%Y)"));
        assert!(!is_valid_branch_template_variable("branch"));
        assert!(!is_valid_branch_template_variable("invalidvar"));
        assert!(!is_valid_branch_template_variable("strftime(invalid"));
    }

    // Triple brace escape syntax tests
    #[test]
    fn test_branch_template_triple_brace_escape() {
        let yaml = r#"
worktree:
  branch_template: "feature/{{{branch}}}-{{commitish}}"
"#;
        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let result = Config::try_from(raw);
        assert!(result.is_ok()); // {{{branch}}} is escape, not a variable
    }

    #[test]
    fn test_expand_branch_template_triple_brace() {
        let env = BranchTemplateEnv {
            commitish: "main".to_string(),
            repository: "repo".to_string(),
        };
        let result = expand_branch_template("{{{branch}}}-{{commitish}}", &env);
        assert_eq!(result, "{{branch}}-main");
    }

    #[test]
    fn test_expand_branch_template_triple_brace_only() {
        let env = BranchTemplateEnv {
            commitish: "main".to_string(),
            repository: "repo".to_string(),
        };
        let result = expand_branch_template("{{{literal}}}", &env);
        assert_eq!(result, "{{literal}}");
    }

    #[test]
    fn test_extract_template_variables_skips_triple_brace() {
        let vars = extract_template_variables("{{{branch}}}-{{commitish}}");
        assert_eq!(vars.len(), 1);
        assert!(vars.contains(&"commitish".to_string()));
        assert!(!vars.contains(&"branch".to_string()));
    }

    #[test]
    fn test_extract_template_variables_triple_brace_only() {
        let vars = extract_template_variables("{{{branch}}}");
        assert!(vars.is_empty());
    }

    // UI config tests

    #[test]
    fn test_ui_show_key_hints_default() {
        let ui = Ui::default();
        assert!(ui.show_key_hints()); // None -> true
    }

    #[test]
    fn test_ui_show_key_hints_explicit_true() {
        let ui = Ui {
            show_key_hints: Some(true),
            ..Default::default()
        };
        assert!(ui.show_key_hints());
    }

    #[test]
    fn test_ui_show_key_hints_explicit_false() {
        let ui = Ui {
            show_key_hints: Some(false),
            ..Default::default()
        };
        assert!(!ui.show_key_hints());
    }

    #[test]
    fn test_ui_add_default_mode_default() {
        let ui = Ui::default();
        assert_eq!(ui.add_default_mode(), AddDefaultMode::New); // None -> New
    }

    #[test]
    fn test_ui_add_default_mode_explicit_existing() {
        let ui = Ui {
            add_default_mode: Some(AddDefaultMode::Existing),
            ..Default::default()
        };
        assert_eq!(ui.add_default_mode(), AddDefaultMode::Existing);
    }

    #[test]
    fn test_ui_add_default_mode_explicit_new() {
        let ui = Ui {
            add_default_mode: Some(AddDefaultMode::New),
            ..Default::default()
        };
        assert_eq!(ui.add_default_mode(), AddDefaultMode::New);
    }

    #[test]
    fn test_parse_ui_show_key_hints_false() {
        let yaml = r#"
ui:
  show_key_hints: false
"#;
        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let config = Config::try_from(raw).unwrap();
        assert_eq!(config.ui.show_key_hints, Some(false));
        assert!(!config.ui.show_key_hints());
    }

    #[test]
    fn test_parse_ui_show_key_hints_true() {
        let yaml = r#"
ui:
  show_key_hints: true
"#;
        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let config = Config::try_from(raw).unwrap();
        assert_eq!(config.ui.show_key_hints, Some(true));
        assert!(config.ui.show_key_hints());
    }

    #[test]
    fn test_parse_ui_add_default_mode_new() {
        let yaml = r#"
ui:
  add_default_mode: new
"#;
        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let config = Config::try_from(raw).unwrap();
        assert_eq!(config.ui.add_default_mode, Some(AddDefaultMode::New));
        assert_eq!(config.ui.add_default_mode(), AddDefaultMode::New);
    }

    #[test]
    fn test_parse_ui_add_default_mode_existing() {
        let yaml = r#"
ui:
  add_default_mode: existing
"#;
        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let config = Config::try_from(raw).unwrap();
        assert_eq!(config.ui.add_default_mode, Some(AddDefaultMode::Existing));
        assert_eq!(config.ui.add_default_mode(), AddDefaultMode::Existing);
    }

    #[test]
    fn test_parse_ui_full_config() {
        let yaml = r##"
ui:
  show_key_hints: false
  add_default_mode: new
  colors:
    border: dark-gray
    accent: "#ff5500"
"##;
        let raw: RawConfig = serde_yaml::from_str(yaml).unwrap();
        let config = Config::try_from(raw).unwrap();
        assert!(!config.ui.show_key_hints());
        assert_eq!(config.ui.add_default_mode(), AddDefaultMode::New);
        assert_eq!(
            config.ui.colors.border,
            Some(UiColor::Named(UiColorName::DarkGray))
        );
        assert_eq!(config.ui.colors.accent, Some(UiColor::Rgb(255, 85, 0)));
    }

    #[test]
    fn test_merge_ui_config_local_overrides_global() {
        let global = Config {
            ui: Ui {
                show_key_hints: Some(false),
                add_default_mode: Some(AddDefaultMode::New),
                ..Default::default()
            },
            ..Default::default()
        };
        let repo = Config {
            ui: Ui {
                show_key_hints: Some(true),
                add_default_mode: Some(AddDefaultMode::Existing),
                ..Default::default()
            },
            ..Default::default()
        };
        let merged = merge_with_global(repo, Some(&global));
        assert!(merged.ui.show_key_hints());
        assert_eq!(merged.ui.add_default_mode(), AddDefaultMode::Existing);
    }

    #[test]
    fn test_merge_ui_config_global_fallback() {
        let global = Config {
            ui: Ui {
                show_key_hints: Some(false),
                add_default_mode: Some(AddDefaultMode::New),
                ..Default::default()
            },
            ..Default::default()
        };
        let repo = Config::default(); // show_key_hints: None, add_default_mode: None
        let merged = merge_with_global(repo, Some(&global));
        // Now global values should be used because repo values are None
        assert!(!merged.ui.show_key_hints());
        assert_eq!(merged.ui.add_default_mode(), AddDefaultMode::New);
    }

    #[test]
    fn test_merge_ui_config_no_global() {
        let repo = Config {
            ui: Ui {
                show_key_hints: Some(false),
                ..Default::default()
            },
            ..Default::default()
        };
        let merged = merge_with_global(repo, None);
        assert!(!merged.ui.show_key_hints());
        assert_eq!(merged.ui.add_default_mode(), AddDefaultMode::New); // None -> default
    }

    // TOML format tests

    #[test]
    fn test_parse_minimal_config_toml() {
        let toml_content = r#"
[[link]]
source = ".env.local"
"#;
        let raw: RawConfig = toml::from_str(toml_content).unwrap();
        let config = Config::try_from(raw).unwrap();
        assert_eq!(config.link.len(), 1);
        assert_eq!(config.link[0].source, PathBuf::from(".env.local"));
        assert_eq!(config.link[0].target, PathBuf::from(".env.local"));
    }

    #[test]
    fn test_parse_full_config_toml() {
        let toml_content = r#"
on_conflict = "skip"

[[mkdir]]
path = "tmp/cache"
description = "Create cache dir"

[[link]]
source = ".env.local"

[[link]]
source = ".secret/creds.json"
target = "config/creds.json"
on_conflict = "abort"
description = "Link credentials"

[[copy]]
source = ".env.example"
target = ".env"
on_conflict = "backup"
"#;
        let raw: RawConfig = toml::from_str(toml_content).unwrap();
        let config = Config::try_from(raw).unwrap();

        assert_eq!(config.on_conflict, Some(OnConflict::Skip));

        assert_eq!(config.mkdir.len(), 1);
        assert_eq!(config.mkdir[0].path, PathBuf::from("tmp/cache"));
        assert_eq!(
            config.mkdir[0].description,
            Some("Create cache dir".to_string())
        );

        assert_eq!(config.link.len(), 2);
        assert_eq!(config.link[1].on_conflict, Some(OnConflict::Abort));
        assert_eq!(
            config.link[1].description,
            Some("Link credentials".to_string())
        );

        assert_eq!(config.copy.len(), 1);
        assert_eq!(config.copy[0].on_conflict, Some(OnConflict::Backup));
    }

    #[test]
    fn test_parse_hooks_toml() {
        let toml_content = r#"
[hooks]
[[hooks.pre_add]]
command = "echo 'pre'"

[[hooks.post_add]]
command = "npm install"
description = "Install dependencies"
"#;
        let raw: RawConfig = toml::from_str(toml_content).unwrap();
        let config = Config::try_from(raw).unwrap();
        assert_eq!(config.hooks.pre_add.len(), 1);
        assert_eq!(config.hooks.pre_add[0].command, "echo 'pre'");
        assert_eq!(config.hooks.post_add.len(), 1);
        assert_eq!(config.hooks.post_add[0].command, "npm install");
        assert_eq!(
            config.hooks.post_add[0].description,
            Some("Install dependencies".to_string())
        );
    }

    #[test]
    fn test_parse_worktree_toml() {
        let toml_content = r#"
[worktree]
path_template = "../worktrees/{{branch}}"
branch_template = "review/{{commitish}}"
"#;
        let raw: RawConfig = toml::from_str(toml_content).unwrap();
        let config = Config::try_from(raw).unwrap();
        assert_eq!(
            config.worktree.path_template,
            Some("../worktrees/{{branch}}".to_string())
        );
        assert_eq!(
            config.worktree.branch_template,
            Some("review/{{commitish}}".to_string())
        );
    }

    #[test]
    fn test_parse_ui_toml() {
        let toml_content = r##"
[ui]
show_key_hints = false
add_default_mode = "existing"

[ui.colors]
border = "dark-gray"
accent = "#ff5500"
"##;
        let raw: RawConfig = toml::from_str(toml_content).unwrap();
        let config = Config::try_from(raw).unwrap();
        assert!(!config.ui.show_key_hints());
        assert_eq!(config.ui.add_default_mode(), AddDefaultMode::Existing);
        assert_eq!(
            config.ui.colors.border,
            Some(UiColor::Named(UiColorName::DarkGray))
        );
        assert_eq!(config.ui.colors.accent, Some(UiColor::Rgb(255, 85, 0)));
    }

    #[test]
    fn test_parse_empty_config_toml() {
        let toml_content = "";
        let raw: RawConfig = toml::from_str(toml_content).unwrap();
        let config = Config::try_from(raw).unwrap();
        assert!(config.link.is_empty());
        assert!(config.copy.is_empty());
        assert!(config.mkdir.is_empty());
    }

    #[test]
    fn test_parse_raw_config_yaml() {
        let yaml = r#"
link:
  - source: ".env"
"#;
        let result = parse_raw_config(yaml, ConfigFormat::Yaml);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_raw_config_toml() {
        let toml_content = r#"
[[link]]
source = ".env"
"#;
        let result = parse_raw_config(toml_content, ConfigFormat::Toml);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_raw_config_invalid_yaml() {
        let yaml = "invalid: yaml: [[[";
        let result = parse_raw_config(yaml, ConfigFormat::Yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_raw_config_invalid_toml() {
        let toml_content = "invalid = [[[";
        let result = parse_raw_config(toml_content, ConfigFormat::Toml);
        assert!(result.is_err());
    }
}
