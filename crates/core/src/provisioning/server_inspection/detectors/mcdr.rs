use std::path::Path;

use super::super::model::{
    EvidenceLocation, EvidenceSource, LaunchPlatform, LaunchProfile, LaunchTarget, ServerComponent,
    ServerComponentKind,
};
use super::{
    ComponentFinding, Findings, ProductFinding, Signal, ecosystems_for_key, product_from_key,
};

/// MCDReforged（MCDR）包裹层目录结构识别。
///
/// 依据 MCDR 官方文档与 #383 讨论共识，MCDR 服务器结构的核心标志是根目录下
/// 同时存在 `config.yml` 与 `permission.yml`；可选的 `plugins/` 插件目录作为
/// 独立佐证。探测只读取文件存在性与目录布局，不执行目录中的任何内容。
pub(super) fn detect(path: &Path, findings: &mut Findings) {
    let config_path = path.join("config.yml");
    let permission_path = path.join("permission.yml");
    if !config_path.is_file() || !permission_path.is_file() {
        return;
    }

    let plugin_dir = path.join("plugins");

    findings.products.push(ProductFinding {
        signal: Signal {
            value: product_from_key("mcdr"),
            detector: "mcdr-directory-layout",
            source: EvidenceSource::DirectoryLayout,
            location: EvidenceLocation::path(config_path.clone()),
            weight: 90,
            correlation_group: "mcdr-config-file",
        },
        ecosystems: ecosystems_for_key("mcdr", false),
    });
    findings.products.push(ProductFinding {
        signal: Signal {
            value: product_from_key("mcdr"),
            detector: "mcdr-directory-layout",
            source: EvidenceSource::DirectoryLayout,
            location: EvidenceLocation::path(permission_path.clone()),
            weight: 90,
            correlation_group: "mcdr-permission-file",
        },
        ecosystems: Vec::new(),
    });
    if plugin_dir.is_dir() {
        findings.products.push(ProductFinding {
            signal: Signal {
                value: product_from_key("mcdr"),
                detector: "mcdr-directory-layout",
                source: EvidenceSource::DirectoryLayout,
                location: EvidenceLocation::path(plugin_dir),
                weight: 60,
                correlation_group: "mcdr-plugin-dir",
            },
            ecosystems: Vec::new(),
        });
    }

    findings.launches.push(Signal {
        value: LaunchProfile {
            id: "mcdr".to_string(),
            platform: LaunchPlatform::Any,
            working_directory: Some(path.to_path_buf()),
            target: LaunchTarget::Command { command: "mcdreforged".to_string() },
            jvm_arguments: Vec::new(),
            program_arguments: Vec::new(),
            required_java_major: None,
        },
        detector: "mcdr-directory-layout",
        source: EvidenceSource::DirectoryLayout,
        location: EvidenceLocation::path(config_path),
        weight: 90,
        correlation_group: "mcdr-config-file",
    });

    findings.components.push(ComponentFinding {
        product_key: "mcdr".to_string(),
        signal: Signal {
            value: ServerComponent {
                kind: ServerComponentKind::Wrapper,
                key: "mcdr".to_string(),
                name: "MCDReforged".to_string(),
                version: None,
                release_channel: None,
                coordinate: None,
                source_path: Some(permission_path),
            },
            detector: "mcdr-directory-layout",
            source: EvidenceSource::DirectoryLayout,
            location: EvidenceLocation::path(path.to_path_buf()),
            weight: 90,
            correlation_group: "mcdr-config-file",
        },
    });
}
