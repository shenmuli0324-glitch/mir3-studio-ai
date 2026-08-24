use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrandConfig {
    pub product_name: String,
    pub core_display_name: String,
    pub identifier: String,
    pub version: String,
    pub user_agent: String,
    pub update_repo: String,
    #[allow(dead_code)]
    pub cli_command: String,
    #[allow(dead_code)]
    pub windows_cli_dir: String,
    pub data_dir: String,
    pub dev_data_dir: String,
    pub home_env: String,
}

pub fn get() -> &'static BrandConfig {
    static BRAND: OnceLock<BrandConfig> = OnceLock::new();
    BRAND.get_or_init(|| {
        serde_json::from_str(include_str!("../../../src/brand.config.json"))
            .expect("MIR3 品牌配置必须是有效 JSON")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_manifests_match_brand_config() {
        let brand = get();
        let tauri: serde_json::Value = serde_json::from_str(include_str!("../../tauri.conf.json"))
            .expect("tauri.conf.json 必须有效");
        let package: serde_json::Value =
            serde_json::from_str(include_str!("../../../package.json"))
                .expect("package.json 必须有效");

        assert_eq!(tauri["productName"], brand.product_name);
        assert_eq!(tauri["identifier"], brand.identifier);
        assert_eq!(tauri["version"], brand.version);
        assert_eq!(package["version"], brand.version);
        assert_eq!(brand.cli_command, "mir3");
        assert_eq!(brand.windows_cli_dir, "mir3-studio-ai");
        assert!(
            include_str!("../../Cargo.toml").contains(&format!("version = \"{}\"", brand.version))
        );
    }
}
