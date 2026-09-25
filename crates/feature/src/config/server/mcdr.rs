//! MCDR（MCDReforged）配置文件管理
//!
//! 提供 `config.yml` 与 `permission.yml` 的读取、写入、解析与预览能力。
//!
//! MCDR 配置文件为 YAML 格式，内部存在列表与嵌套映射（如插件目录、权限组）。
//! 为兼顾小白编辑与结构安全，本管理器只对**顶层简单标量键**提供可视化编辑：
//! 写回时保留注释、顺序与嵌套结构；嵌套块的顶层键（`key:` 后无值）不会被替换，
//! 避免破坏 YAML 层级。

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use sealantern_contract::server_config::{ConfigEntry, McdrConfig, McdrConfigFile};
use sealantern_infra::fs::{FileLock, FsError, write_atomic_blocking};
use tracing::debug;

/// MCDR 配置文件管理器
pub struct McdrConfigManager {
    server_path: std::path::PathBuf,
}

impl McdrConfigManager {
    pub fn new(server_path: impl AsRef<Path>) -> Self {
        Self {
            server_path: server_path.as_ref().to_path_buf(),
        }
    }

    /// 获取指定角色的 MCDR 配置文件路径
    fn config_file(&self, file: McdrConfigFile) -> std::path::PathBuf {
        self.server_path.join(file.file_name())
    }

    /// 读取 MCDR 配置文件为可视化配置结构
    pub fn read(&self, file: McdrConfigFile) -> Result<McdrConfig, McdrConfigError> {
        let file_path = self.config_file(file);

        if !file_path.exists() {
            debug!("MCDR 配置文件不存在: {:?}", file_path);
            return Ok(McdrConfig {
                file,
                entries: vec![],
                raw: BTreeMap::new(),
            });
        }

        let content = fs::read_to_string(&file_path)?;
        Self::parse_source(file, &content)
    }

    /// 按键值对更新 MCDR 配置文件（保留注释、顺序与嵌套结构）
    pub fn write(
        &self,
        file: McdrConfigFile,
        values: &BTreeMap<String, String>,
    ) -> Result<(), McdrConfigError> {
        let file_path = self.config_file(file);
        let _lock = lock_config_file(&file_path)?;

        let source = read_source_or_default(&file_path)?;
        let content = apply_values_to_source(&source, values);
        write_config_file(&file_path, &content)?;

        debug!("写入 MCDR 配置文件成功: {:?}", file_path);
        Ok(())
    }

    /// 读取 MCDR 配置文件原始文本
    pub fn read_source(&self, file: McdrConfigFile) -> Result<String, McdrConfigError> {
        let file_path = self.config_file(file);
        let content = fs::read_to_string(&file_path)?;
        Ok(content)
    }

    /// 写入 MCDR 配置文件原始文本
    pub fn write_source(&self, file: McdrConfigFile, source: &str) -> Result<(), McdrConfigError> {
        let file_path = self.config_file(file);
        let _lock = lock_config_file(&file_path)?;
        write_config_file(&file_path, source)?;
        debug!("写入 MCDR 配置文件原始文本成功: {:?}", file_path);
        Ok(())
    }

    /// 解析 MCDR 配置文件原始文本为可视化配置结构
    pub fn parse_source(file: McdrConfigFile, source: &str) -> Result<McdrConfig, McdrConfigError> {
        let raw = parse_flat_yaml(source);
        let entries = raw
            .iter()
            .map(|(key, value)| {
                let metadata = key_metadata(key, value);
                ConfigEntry {
                    key: key.clone(),
                    value: value.clone(),
                    description: String::new(),
                    value_type: metadata.value_type.to_string(),
                    default_value: metadata.default_value.to_string(),
                    category: metadata.category.to_string(),
                }
            })
            .collect();

        Ok(McdrConfig { file, entries, raw })
    }

    /// 预览可视化配置写回后的最终文本（基于服务器目录下的现有内容）
    pub fn preview_write(
        &self,
        file: McdrConfigFile,
        values: &BTreeMap<String, String>,
    ) -> Result<String, McdrConfigError> {
        let file_path = self.config_file(file);
        let source = read_source_or_default(&file_path)?;
        Ok(apply_values_to_source(&source, values))
    }

    /// 基于给定源码预览可视化配置写回后的最终文本
    pub fn preview_write_from_source(
        source: &str,
        values: &BTreeMap<String, String>,
    ) -> Result<String, McdrConfigError> {
        Ok(apply_values_to_source(source, values))
    }
}

const DEFAULT_SOURCE: &str = "#MCDReforged config";

#[derive(Clone, Copy)]
struct KeyMetadata {
    value_type: &'static str,
    default_value: &'static str,
    category: &'static str,
}

/// 仅为编辑器已支持的常见 MCDR 键提供小型 schema；未知键仍保持可编辑字符串。
fn key_metadata(key: &str, value: &str) -> KeyMetadata {
    let value_type = match key {
        "advanced_console"
        | "debug"
        | "check_updates"
        | "disable_console_width_detection"
        | "honor_plugin_ordering"
        | "permanent_redirect_stdout"
        | "disable_plugin_metadata_cache"
        | "compact_console" => "boolean",
        "working_directory" | "plugin_directories" | "language" | "start_command"
        | "console_command" | "command_prefix" => "string",
        _ if matches!(value, "true" | "false") => "boolean",
        _ if value.chars().all(|c| c.is_ascii_digit()) => "number",
        _ => "string",
    };

    let category = match key {
        "working_directory" | "plugin_directories" | "language" | "start_command" => "basic",
        "advanced_console" | "debug" | "check_updates" | "permanent_redirect_stdout" => "debug",
        "console_command" | "command_prefix" | "compact_console" => "console",
        "default_permission" => "permission",
        _ => "other",
    };

    let default_value = match key {
        "working_directory" => ".",
        "language" => "en_us",
        "default_permission" => "normal",
        _ => "",
    };

    KeyMetadata { value_type, default_value, category }
}

/// 解析 YAML 顶层简单标量键值。
///
/// 跳过注释、空行、缩进行（嵌套）与 `- ` 列表项；`key:` 空值的嵌套块头也不收集。
fn parse_flat_yaml(source: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();

    for line in source.lines() {
        let trimmed_start = line.trim_start();
        if line.starts_with(' ') || line.starts_with('\t') {
            continue;
        }
        if trimmed_start.is_empty()
            || trimmed_start.starts_with('#')
            || trimmed_start.starts_with('!')
        {
            continue;
        }
        let Some(colon) = line.find(':') else {
            continue;
        };
        let key = line[..colon].trim();
        let raw_value = line[colon + 1..].trim();
        if key.is_empty() || raw_value.is_empty() || raw_value.starts_with('-') {
            continue;
        }
        map.insert(key.to_string(), unquote_scalar(raw_value));
    }

    map
}

/// 去除 YAML 简单标量的成对首尾引号。
fn unquote_scalar(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        if (bytes[0] == b'"' && bytes[value.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[value.len() - 1] == b'\'')
        {
            return value[1..value.len() - 1].to_string();
        }
    }
    value.to_string()
}

fn read_source_or_default(path: &Path) -> Result<String, McdrConfigError> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(DEFAULT_SOURCE.to_owned()),
        Err(error) => Err(error.into()),
    }
}

fn apply_values_to_source(source: &str, values: &BTreeMap<String, String>) -> String {
    let mut lines = source.lines().map(str::to_owned).collect::<Vec<_>>();

    for (key, value) in values {
        // 只更新顶层"简单标量"行（`key: value`）；嵌套块头与列表行不替换。
        // 重复键与解析规则一致：后者生效，因此写回也更新最后一个匹配项。
        if let Some(index) = lines
            .iter()
            .rposition(|line| line_matches_top_level_key(line, key))
        {
            lines[index] = format!("{key}: {value}");
        } else {
            lines.push(format!("{key}: {value}"));
        }
    }

    lines.join("\n")
}

/// 判断一行是否为指定键的顶层简单标量行。
fn line_matches_top_level_key(line: &str, key: &str) -> bool {
    if line.starts_with(' ') || line.starts_with('\t') {
        return false;
    }
    let trimmed_start = line.trim_start();
    if trimmed_start.starts_with('#') || trimmed_start.starts_with('!') {
        return false;
    }
    let Some(colon) = line.find(':') else {
        return false;
    };
    let parsed_key = line[..colon].trim();
    let raw_value = line[colon + 1..].trim();
    parsed_key == key && !raw_value.is_empty() && !raw_value.starts_with('-')
}

fn lock_config_file(path: &Path) -> Result<FileLock, McdrConfigError> {
    FileLock::try_acquire(path).map_err(storage_error)
}

fn write_config_file(path: &Path, content: &str) -> Result<(), McdrConfigError> {
    write_atomic_blocking(path, content.as_bytes()).map_err(storage_error)
}

fn storage_error(error: FsError) -> McdrConfigError {
    std::io::Error::other(error.to_string()).into()
}

/// MCDR 配置文件处理错误
#[derive(Debug, thiserror::Error)]
pub enum McdrConfigError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("解析错误: {0}")]
    Parse(String),
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use sealantern_contract::server_config::McdrConfigFile;

    use super::*;

    #[test]
    fn parse_collects_top_level_scalars_and_skips_nested_blocks() {
        let source = r#"
# MCDReforged config
working_directory: .
plugin_directories: ['plugins']
debug: false
advanced_console: true

plugins:
  example_plugin:
    enabled: true
command_prefix: '!!'
"#;
        let config = McdrConfigManager::parse_source(McdrConfigFile::Config, source)
            .expect("parse should succeed");

        assert_eq!(config.raw.get("working_directory").map(String::as_str), Some("."));
        assert_eq!(config.raw.get("debug").map(String::as_str), Some("false"));
        assert_eq!(config.raw.get("advanced_console").map(String::as_str), Some("true"));
        assert_eq!(config.raw.get("command_prefix").map(String::as_str), Some("!!"));
        // 嵌套块头与嵌套键不进入顶层视图
        assert!(config.raw.get("plugins").is_none());
        assert!(config.raw.get("enabled").is_none());

        let by_key = config
            .entries
            .iter()
            .map(|entry| (entry.key.as_str(), entry))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(by_key["debug"].value_type, "boolean");
        assert_eq!(by_key["working_directory"].default_value, ".");
    }

    #[test]
    fn apply_updates_scalar_lines_and_appends_missing_keys() {
        let source = "working_directory: .\n# keep\ndebug: false\n";
        let mut values = BTreeMap::new();
        values.insert("debug".to_string(), "true".to_string());
        values.insert("language".to_string(), "zh_cn".to_string());

        let output = McdrConfigManager::preview_write_from_source(source, &values)
            .expect("preview should succeed");

        assert_eq!(output, "working_directory: .\n# keep\ndebug: true\nlanguage: zh_cn");
    }

    #[test]
    fn apply_does_not_break_nested_block_headers() {
        let source = "plugins:\n  example:\n    enabled: true\nstart_command: ''\n";
        let mut values = BTreeMap::new();
        values.insert("plugins".to_string(), "['plugins']".to_string());

        // `plugins:` 是嵌套块头（值空），不会被就地替换；但按“重复键后者生效”
        // 的规则，会追加一个顶层 `plugins: ['plugins']` 显式覆盖该键。
        let output = McdrConfigManager::preview_write_from_source(source, &values)
            .expect("preview should succeed");

        assert!(output.starts_with("plugins:\n  example:\n    enabled: true\n"));
        assert!(output.ends_with("plugins: ['plugins']"));
        let parsed = parse_flat_yaml(&output);
        assert_eq!(parsed.get("plugins").map(String::as_str), Some("['plugins']"));
        assert_eq!(parsed.get("start_command").map(String::as_str), Some(""));
    }

    #[test]
    fn file_write_is_atomic_and_releases_the_lock() {
        let root = tempfile::tempdir().expect("temporary server directory should be created");
        let manager = McdrConfigManager::new(root.path());

        let mut values = BTreeMap::new();
        values.insert("debug".to_string(), "true".to_string());
        manager
            .write(McdrConfigFile::Config, &values)
            .expect("config write should succeed");

        let path = root.path().join("config.yml");
        assert_eq!(
            std::fs::read_to_string(&path).expect("config.yml should be readable"),
            "#MCDReforged config\ndebug: true"
        );
        let lock = FileLock::try_acquire(&path).expect("write should release the file lock");
        drop(lock);
    }

    #[test]
    fn permission_file_round_trips_source() {
        let root = tempfile::tempdir().expect("temporary server directory should be created");
        let manager = McdrConfigManager::new(root.path());

        manager
            .write_source(
                McdrConfigFile::Permission,
                "default_permission: normal\n# players\nsteve: operator\n",
            )
            .expect("permission write should succeed");

        let config = manager
            .read(McdrConfigFile::Permission)
            .expect("permission read should succeed");
        assert_eq!(config.raw.get("default_permission").map(String::as_str), Some("normal"));
        assert_eq!(config.raw.get("steve").map(String::as_str), Some("operator"));
    }
}
