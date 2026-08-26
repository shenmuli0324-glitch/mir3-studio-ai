fn main() {
    println!("cargo:rerun-if-env-changed=MIR3_DOMAIN_PACK_INDEX_URL");
    println!("cargo:rerun-if-env-changed=MIR3_DOMAIN_PACK_ED25519_PUBLIC_KEY");
    validate_domain_update_build_config();
    tauri_build::build()
}

/// 发布配置必须成对注入，且在编译期拒绝明文源、凭据 URL 与明显无效的公钥。
fn validate_domain_update_build_config() {
    let index = std::env::var("MIR3_DOMAIN_PACK_INDEX_URL").ok();
    let key = std::env::var("MIR3_DOMAIN_PACK_ED25519_PUBLIC_KEY").ok();
    match (index, key) {
        (None, None) => {}
        (Some(index), Some(key)) => {
            let index = index.trim();
            assert!(
                index.starts_with("https://")
                    && !index.contains('@')
                    && !index.chars().any(char::is_whitespace),
                "DOMAIN_PACK_UPDATE_BUILD_CONFIG_INVALID: index must be a credential-free HTTPS URL"
            );
            let key = key.trim();
            assert!(
                matches!(key.len(), 43 | 44)
                    && key
                        .chars()
                        .all(|value| value.is_ascii_alphanumeric() || matches!(value, '+' | '/' | '=')),
                "DOMAIN_PACK_UPDATE_BUILD_CONFIG_INVALID: public key must be Base64-encoded Ed25519 bytes"
            );
        }
        _ => panic!(
            "DOMAIN_PACK_UPDATE_BUILD_CONFIG_INCOMPLETE: index URL and Ed25519 public key must be provided together"
        ),
    }
}
