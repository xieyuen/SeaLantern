//! 服务器配置（server.properties）契约模型。
//!
//! 定义宿主消费的配置条目与配置结构等模型，全部可序列化，供跨传输面传递。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// 配置条目信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigEntry {
    /// 配置键
    pub key: String,
    /// 配置值
    pub value: String,
    /// 配置项描述
    pub description: String,
    /// 值类型（`number` / `boolean` / `string`）
    pub value_type: String,
    /// 默认值
    pub default_value: String,
    /// 配置分组（`network` / `player` / `game` / `world` / `performance` / `display` / `other`）
    pub category: String,
}

/// 服务器配置文件结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerProperties {
    /// 可视化配置条目列表
    pub entries: Vec<ConfigEntry>,
    /// 原始键值对
    pub raw: BTreeMap<String, String>,
}

/// MCDR（MCDReforged）配置文件角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McdrConfigFile {
    /// `config.yml`：MCDR 主配置。
    Config,
    /// `permission.yml`：MCDR 权限配置。
    Permission,
}

impl McdrConfigFile {
    pub fn file_name(self) -> &'static str {
        match self {
            Self::Config => "config.yml",
            Self::Permission => "permission.yml",
        }
    }
}

/// MCDR 配置文件结构（`config.yml` / `permission.yml` 的顶层键值视图）。
///
/// 顶层简单标量键可编辑；嵌套结构（列表、映射）保留在原始文本中，避免破坏 YAML 层级。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McdrConfig {
    /// 文件角色。
    pub file: McdrConfigFile,
    /// 可视化配置条目列表（顶层标量键）。
    pub entries: Vec<ConfigEntry>,
    /// 原始顶层键值对。
    pub raw: BTreeMap<String, String>,
}
