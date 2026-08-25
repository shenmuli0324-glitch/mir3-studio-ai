use crate::model::{DeviceKind, SceneCatalogEntry};

pub fn catalog() -> Vec<SceneCatalogEntry> {
    vec![
        entry(
            "character-create",
            "人物创建",
            "人物创建背景、职业选择与角色预览的组合场景",
            DeviceKind::Mobile,
            "GUIExport/login_role/login_role_create.lua",
        ),
        entry(
            "character-select",
            "人物选择",
            "人物选择背景、角色槽位与操作按钮的组合场景",
            DeviceKind::Mobile,
            "GUIExport/login_role/login_role.lua",
        ),
        entry(
            "game-mobile",
            "移动端主界面",
            "移动端完整 HUD、窗口栈与脱机模拟世界场景",
            DeviceKind::Mobile,
            "GUILayout/GUIInit.lua",
        ),
        entry(
            "game-pc",
            "PC 主界面",
            "PC 完整 HUD、窗口栈与脱机模拟世界场景",
            DeviceKind::Pc,
            "GUILayout/GUIInit.lua",
        ),
    ]
}

fn entry(
    id: &str,
    title: &str,
    description: &str,
    recommended_device: DeviceKind,
    entry_hint: &str,
) -> SceneCatalogEntry {
    SceneCatalogEntry {
        id: id.to_string(),
        title: title.to_string(),
        description: description.to_string(),
        recommended_device,
        entry_hint: entry_hint.to_string(),
    }
}

pub fn source(scene_id: &str) -> Option<&'static str> {
    match scene_id {
        "game-mobile" | "hud-mobile" => Some(
            r#"local root = GUI:Node_Create(parent, "MobileHud", 0, 0)
GUI:Image_Create(root, "Avatar", 24, 548, "res/private/main/role.png")
GUI:Button_Create(root, "BagButton", 1040, 36, "res/private/main/bag.png")
return root"#,
        ),
        "game-pc" | "hud-pc" => Some(
            r#"local root = GUI:Node_Create(parent, "PcHud", 0, 0)
GUI:Image_Create(root, "Status", 16, 700, "res/private/main/status.png")
GUI:Layout_Create(root, "ShortcutBar", 332, 18, 360, 56, false)
return root"#,
        ),
        "character-select" | "login" => Some(
            r#"local root = GUI:Layout_Create(parent, "LoginRoot", 0, 0, 1136, 640, false)
GUI:Image_Create(root, "Background", 568, 320, "res/private/login/bg_cjzy_02.png")
GUI:Button_Create(root, "Create", 1008, 146, "res/private/login/za_1.png")
return root"#,
        ),
        "character-create" => Some(
            r#"local root = GUI:Layout_Create(parent, "CreateRoot", 0, 0, 1136, 640, false)
GUI:Image_Create(root, "Background", 568, 320, "res/private/login/create/img_bg.png")
GUI:Button_Create(root, "Submit", 638, 70, "res/private/login/create/btn_ok2.png")
return root"#,
        ),
        _ => None,
    }
}
