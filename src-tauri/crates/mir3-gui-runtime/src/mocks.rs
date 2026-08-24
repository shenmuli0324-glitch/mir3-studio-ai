use crate::model::{DeviceKind, SceneCatalogEntry};

pub fn catalog() -> Vec<SceneCatalogEntry> {
    vec![
        entry(
            "auction",
            "拍卖行",
            "拍卖列表、筛选与竞价流程的静态运行场景",
            DeviceKind::Mobile,
            "GUILayout/auction/auction_main.lua",
        ),
        entry(
            "bag",
            "背包",
            "角色背包与物品格的静态运行场景",
            DeviceKind::Mobile,
            "GUILayout/player_bag/bag.lua",
        ),
        entry(
            "hud-mobile",
            "移动端主界面",
            "移动端 HUD 与常用入口的静态运行场景",
            DeviceKind::Mobile,
            "GUILayout/GUIInit.lua",
        ),
        entry(
            "hud-pc",
            "PC 主界面",
            "PC HUD 与快捷栏的静态运行场景",
            DeviceKind::Pc,
            "GUILayout/GUIInit_win32.lua",
        ),
        entry(
            "login",
            "登录",
            "登录和角色选择流程的静态运行场景",
            DeviceKind::Mobile,
            "GUILayout/login/login.lua",
        ),
        entry(
            "store",
            "商城",
            "商城分类、商品与购买流程的静态运行场景",
            DeviceKind::Mobile,
            "GUILayout/store/store.lua",
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
        "auction" => Some(
            r#"local root = GUI:Layout_Create(parent, "AuctionRoot", 188, 90, 760, 500, false)
GUI:Layout_setBackGroundImage(root, "res/private/auction/bg.png")
local title = GUI:Text_Create(root, "Title", 36, 450, 22, "拍卖行")
local list = GUI:ListView_Create(root, "AuctionList", 30, 65, 700, 360, 1)
return root"#,
        ),
        "bag" => Some(
            r#"local root = GUI:Layout_Create(parent, "BagRoot", 620, 80, 470, 520, false)
local title = GUI:Text_Create(root, "Title", 24, 475, 22, "背包")
local items = GUI:ListView_Create(root, "Items", 24, 80, 420, 370, 1)
return root"#,
        ),
        "hud-mobile" => Some(
            r#"local root = GUI:Node_Create(parent, "MobileHud", 0, 0)
GUI:Image_Create(root, "Avatar", 24, 548, "res/private/main/role.png")
GUI:Button_Create(root, "BagButton", 1040, 36, "res/private/main/bag.png")
return root"#,
        ),
        "hud-pc" => Some(
            r#"local root = GUI:Node_Create(parent, "PcHud", 0, 0)
GUI:Image_Create(root, "Status", 16, 700, "res/private/main/status.png")
GUI:Layout_Create(root, "ShortcutBar", 332, 18, 360, 56, false)
return root"#,
        ),
        "login" => Some(
            r#"local root = GUI:Layout_Create(parent, "LoginRoot", 0, 0, 1136, 640, false)
GUI:Image_Create(root, "Background", 0, 0, "res/private/login/background.jpg")
GUI:Button_Create(root, "Enter", 488, 86, "res/private/login/enter.png")
return root"#,
        ),
        "store" => Some(
            r#"local root = GUI:Layout_Create(parent, "StoreRoot", 188, 80, 760, 520, false)
GUI:Text_Create(root, "Title", 30, 475, 22, "商城")
GUI:ListView_Create(root, "Goods", 190, 70, 540, 370, 1)
return root"#,
        ),
        _ => None,
    }
}
