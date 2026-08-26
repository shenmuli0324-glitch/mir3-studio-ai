//! MIR3 地图二进制的无损解析与受控编辑。
//!
//! 当前只对已经通过语料验证的 28 字节头、59 字节 2×2 块格式开放写入。
//! 未识别格式仍返回稳定诊断，避免错误猜测造成地图损坏。

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const HEADER_LEN: usize = 28;
const CELL_LEN: usize = 14;
const BACK_BLOCK_LEN: usize = 3;
const MAX_DIMENSION: usize = 2048;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MapFormat {
    Aragom31,
    Mir3ZeroHeader,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MapCapabilities {
    pub terrain_editable: bool,
    pub background: bool,
    pub middle: bool,
    pub front: bool,
    pub collision: bool,
    pub doors: bool,
    pub animation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MapDiagnostic {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MapHeader {
    pub format: MapFormat,
    pub width: u16,
    pub height: u16,
    pub source_sha256: String,
    pub capabilities: MapCapabilities,
    pub diagnostics: Vec<MapDiagnostic>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MapSpriteRef {
    pub library: i16,
    pub image: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MapCell {
    pub x: u16,
    pub y: u16,
    pub background: MapSpriteRef,
    pub middle: MapSpriteRef,
    pub front: MapSpriteRef,
    pub walkable: bool,
    pub front_blocked: bool,
    pub middle_animation_frames: u8,
    pub front_animation_frames: u8,
    pub door_index: u8,
    pub door_offset: u8,
    pub light: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MapChunk {
    pub chunk_x: u16,
    pub chunk_y: u16,
    pub start_x: u16,
    pub start_y: u16,
    pub width: u16,
    pub height: u16,
    pub cells: Vec<MapCell>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MapLayer {
    Background,
    Middle,
    Front,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum MapEditOperation {
    SetSprite {
        x: u16,
        y: u16,
        layer: MapLayer,
        library: i16,
        image: u16,
    },
    ClearSprite {
        x: u16,
        y: u16,
        layer: MapLayer,
    },
    SetCollision {
        x: u16,
        y: u16,
        walkable: bool,
        front_blocked: bool,
    },
    SetDoor {
        x: u16,
        y: u16,
        door_index: u8,
        door_offset: u8,
    },
    SetAnimation {
        x: u16,
        y: u16,
        middle_frames: u8,
        front_frames: u8,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NewMapOptions {
    pub width: u16,
    pub height: u16,
    pub background_library: i16,
    pub background_image: u16,
    pub walkable: bool,
}

#[derive(Debug, Clone)]
pub struct MapDocument {
    bytes: Vec<u8>,
    header: MapHeader,
}

impl MapDocument {
    pub fn parse(bytes: Vec<u8>) -> Result<Self, String> {
        let header = detect_header(&bytes);
        if header.format == MapFormat::Unknown {
            return Ok(Self { bytes, header });
        }
        validate_layout(&bytes, header.width, header.height)?;
        Ok(Self { bytes, header })
    }

    pub fn header(&self) -> &MapHeader {
        &self.header
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub fn chunk(&self, chunk_x: u16, chunk_y: u16, chunk_size: u16) -> Result<MapChunk, String> {
        self.ensure_editable()?;
        if chunk_size == 0 || chunk_size > 128 {
            return Err("MAP_CHUNK_SIZE_INVALID: chunk size must be between 1 and 128".to_string());
        }
        let start_x = usize::from(chunk_x) * usize::from(chunk_size);
        let start_y = usize::from(chunk_y) * usize::from(chunk_size);
        let width = usize::from(self.header.width);
        let height = usize::from(self.header.height);
        if start_x >= width || start_y >= height {
            return Err("MAP_CHUNK_OUTSIDE: chunk is outside map bounds".to_string());
        }
        let end_x = (start_x + usize::from(chunk_size)).min(width);
        let end_y = (start_y + usize::from(chunk_size)).min(height);
        let mut cells = Vec::with_capacity((end_x - start_x) * (end_y - start_y));
        for y in start_y..end_y {
            for x in start_x..end_x {
                cells.push(self.cell(x as u16, y as u16)?);
            }
        }
        Ok(MapChunk {
            chunk_x,
            chunk_y,
            start_x: start_x as u16,
            start_y: start_y as u16,
            width: (end_x - start_x) as u16,
            height: (end_y - start_y) as u16,
            cells,
        })
    }

    pub fn cell(&self, x: u16, y: u16) -> Result<MapCell, String> {
        self.ensure_editable()?;
        ensure_coordinate(self.header.width, self.header.height, x, y)?;
        let back = back_offset(self.header.width, self.header.height, x, y);
        let cell = cell_offset(self.header.width, self.header.height, x, y);
        let flag = self.bytes[cell];
        Ok(MapCell {
            x,
            y,
            background: decode_sprite(self.bytes[back], read_u16(&self.bytes, back + 1)),
            middle: decode_sprite(self.bytes[cell + 4], read_u16(&self.bytes, cell + 5)),
            front: decode_sprite(self.bytes[cell + 3], read_u16(&self.bytes, cell + 7)),
            walkable: flag & 0x01 == 0x01,
            front_blocked: flag & 0x02 != 0x02,
            middle_animation_frames: self.bytes[cell + 1],
            front_animation_frames: self.bytes[cell + 2] & 0x8f,
            door_index: self.bytes[cell + 9],
            door_offset: self.bytes[cell + 10],
            light: self.bytes[cell + 12] & 0x0f,
        })
    }

    pub fn apply(&mut self, operations: &[MapEditOperation]) -> Result<(), String> {
        self.ensure_editable()?;
        for operation in operations {
            self.apply_one(operation)?;
        }
        self.header.source_sha256 = hash_bytes(&self.bytes);
        Ok(())
    }

    pub fn clone_cleared(&self, options: NewMapOptions) -> Result<Self, String> {
        self.ensure_editable()?;
        validate_dimensions(options.width, options.height)?;
        let blocks = back_block_count(options.width, options.height);
        let cells = usize::from(options.width) * usize::from(options.height);
        let mut bytes = vec![0_u8; HEADER_LEN + blocks * BACK_BLOCK_LEN + cells * CELL_LEN];
        bytes[..HEADER_LEN].copy_from_slice(&self.bytes[..HEADER_LEN]);
        write_u16(&mut bytes, 22, options.width);
        write_u16(&mut bytes, 24, options.height);
        let encoded = encode_sprite(options.background_library, options.background_image)?;
        for index in 0..blocks {
            let offset = HEADER_LEN + index * BACK_BLOCK_LEN;
            bytes[offset] = encoded.0;
            write_u16(&mut bytes, offset + 1, encoded.1);
        }
        let cells_start = HEADER_LEN + blocks * BACK_BLOCK_LEN;
        for index in 0..cells {
            let offset = cells_start + index * CELL_LEN;
            bytes[offset] = if options.walkable { 0x03 } else { 0x00 };
            bytes[offset + 3] = 0xff;
            bytes[offset + 4] = 0xff;
        }
        Self::parse(bytes)
    }

    fn apply_one(&mut self, operation: &MapEditOperation) -> Result<(), String> {
        match *operation {
            MapEditOperation::SetSprite {
                x,
                y,
                layer,
                library,
                image,
            } => {
                ensure_coordinate(self.header.width, self.header.height, x, y)?;
                let encoded = encode_sprite(library, image)?;
                match layer {
                    MapLayer::Background => {
                        let offset = back_offset(self.header.width, self.header.height, x, y);
                        self.bytes[offset] = encoded.0;
                        write_u16(&mut self.bytes, offset + 1, encoded.1);
                    }
                    MapLayer::Middle => {
                        let offset = cell_offset(self.header.width, self.header.height, x, y);
                        self.bytes[offset + 4] = encoded.0;
                        write_u16(&mut self.bytes, offset + 5, encoded.1);
                    }
                    MapLayer::Front => {
                        let offset = cell_offset(self.header.width, self.header.height, x, y);
                        self.bytes[offset + 3] = encoded.0;
                        write_u16(&mut self.bytes, offset + 7, encoded.1);
                    }
                }
            }
            MapEditOperation::ClearSprite { x, y, layer } => {
                self.apply_one(&MapEditOperation::SetSprite {
                    x,
                    y,
                    layer,
                    library: -1,
                    image: 0,
                })?;
            }
            MapEditOperation::SetCollision {
                x,
                y,
                walkable,
                front_blocked,
            } => {
                ensure_coordinate(self.header.width, self.header.height, x, y)?;
                let offset = cell_offset(self.header.width, self.header.height, x, y);
                let mut flag = self.bytes[offset] & !0x03;
                if walkable {
                    flag |= 0x01;
                }
                if !front_blocked {
                    flag |= 0x02;
                }
                self.bytes[offset] = flag;
            }
            MapEditOperation::SetDoor {
                x,
                y,
                door_index,
                door_offset,
            } => {
                ensure_coordinate(self.header.width, self.header.height, x, y)?;
                let offset = cell_offset(self.header.width, self.header.height, x, y);
                self.bytes[offset + 9] = door_index;
                self.bytes[offset + 10] = door_offset;
            }
            MapEditOperation::SetAnimation {
                x,
                y,
                middle_frames,
                front_frames,
            } => {
                ensure_coordinate(self.header.width, self.header.height, x, y)?;
                let offset = cell_offset(self.header.width, self.header.height, x, y);
                self.bytes[offset + 1] = middle_frames;
                self.bytes[offset + 2] = front_frames & 0x8f;
            }
        }
        Ok(())
    }

    fn ensure_editable(&self) -> Result<(), String> {
        if self.header.capabilities.terrain_editable {
            Ok(())
        } else {
            Err("MAP_FORMAT_READ_ONLY: the detected map format is not editable".to_string())
        }
    }
}

pub fn detect_header(bytes: &[u8]) -> MapHeader {
    detect_header_with_len(bytes, bytes.len(), Some(hash_bytes(bytes)))
}

/// 目录扫描只读取 28 字节头，避免为上千张地图反复读取完整文件。
pub fn detect_header_with_len(
    prefix: &[u8],
    file_len: usize,
    source_sha256: Option<String>,
) -> MapHeader {
    let source_sha256 = source_sha256.unwrap_or_default();
    if prefix.len() < HEADER_LEN {
        return unknown_header(
            source_sha256,
            0,
            0,
            "MAP_HEADER_TRUNCATED",
            "地图文件不足 28 字节",
        );
    }
    let width = read_u16(prefix, 22);
    let height = read_u16(prefix, 24);
    let format = if prefix.starts_with(b"Aragom") {
        MapFormat::Aragom31
    } else if prefix[..20].iter().all(|byte| *byte == 0) {
        MapFormat::Mir3ZeroHeader
    } else {
        MapFormat::Unknown
    };
    if format == MapFormat::Unknown {
        return unknown_header(
            source_sha256,
            width,
            height,
            "MAP_FORMAT_UNSUPPORTED",
            "地图格式尚未纳入可写适配器",
        );
    }
    let expected = expected_len(width, height);
    if validate_dimensions(width, height).is_err() || expected != Some(file_len) {
        return unknown_header(
            source_sha256,
            width,
            height,
            "MAP_LAYOUT_MISMATCH",
            "地图头尺寸与 59 字节块布局不匹配，已降级为只读",
        );
    }
    MapHeader {
        format,
        width,
        height,
        source_sha256,
        capabilities: editable_capabilities(),
        diagnostics: Vec::new(),
    }
}

fn unknown_header(
    source_sha256: String,
    width: u16,
    height: u16,
    code: &str,
    message: &str,
) -> MapHeader {
    MapHeader {
        format: MapFormat::Unknown,
        width,
        height,
        source_sha256,
        capabilities: MapCapabilities {
            terrain_editable: false,
            background: false,
            middle: false,
            front: false,
            collision: false,
            doors: false,
            animation: false,
        },
        diagnostics: vec![MapDiagnostic {
            code: code.to_string(),
            message: message.to_string(),
        }],
    }
}

fn editable_capabilities() -> MapCapabilities {
    MapCapabilities {
        terrain_editable: true,
        background: true,
        middle: true,
        front: true,
        collision: true,
        doors: true,
        animation: true,
    }
}

fn validate_layout(bytes: &[u8], width: u16, height: u16) -> Result<(), String> {
    validate_dimensions(width, height)?;
    if expected_len(width, height) == Some(bytes.len()) {
        Ok(())
    } else {
        Err("MAP_LAYOUT_MISMATCH: map byte length does not match dimensions".to_string())
    }
}

fn validate_dimensions(width: u16, height: u16) -> Result<(), String> {
    if width == 0
        || height == 0
        || width % 2 != 0
        || height % 2 != 0
        || usize::from(width) > MAX_DIMENSION
        || usize::from(height) > MAX_DIMENSION
    {
        return Err(
            "MAP_DIMENSIONS_INVALID: width and height must be even values from 2 to 2048"
                .to_string(),
        );
    }
    Ok(())
}

fn expected_len(width: u16, height: u16) -> Option<usize> {
    let blocks = back_block_count(width, height);
    let cells = usize::from(width).checked_mul(usize::from(height))?;
    HEADER_LEN
        .checked_add(blocks.checked_mul(BACK_BLOCK_LEN)?)?
        .checked_add(cells.checked_mul(CELL_LEN)?)
}

fn back_block_count(width: u16, height: u16) -> usize {
    usize::from(width.div_ceil(2)) * usize::from(height.div_ceil(2))
}

fn back_offset(_width: u16, height: u16, x: u16, y: u16) -> usize {
    let height_blocks = usize::from(height.div_ceil(2));
    let index = usize::from(x / 2) * height_blocks + usize::from(y / 2);
    HEADER_LEN + index * BACK_BLOCK_LEN
}

fn cell_offset(width: u16, height: u16, x: u16, y: u16) -> usize {
    let cells_start = HEADER_LEN + back_block_count(width, height) * BACK_BLOCK_LEN;
    let index = usize::from(x) * usize::from(height) + usize::from(y);
    cells_start + index * CELL_LEN
}

fn ensure_coordinate(width: u16, height: u16, x: u16, y: u16) -> Result<(), String> {
    if x < width && y < height {
        Ok(())
    } else {
        Err("MAP_COORDINATE_OUTSIDE: coordinate is outside map bounds".to_string())
    }
}

fn decode_sprite(library: u8, stored_image: u16) -> MapSpriteRef {
    if library == 0xff {
        MapSpriteRef {
            library: -1,
            image: 0,
        }
    } else {
        MapSpriteRef {
            library: i16::from(library),
            image: stored_image.saturating_add(1),
        }
    }
}

fn encode_sprite(library: i16, image: u16) -> Result<(u8, u16), String> {
    if library == -1 || image == 0 {
        return Ok((0xff, 0));
    }
    let library = u8::try_from(library)
        .map_err(|_| "MAP_SPRITE_LIBRARY_INVALID: library must be between 0 and 254".to_string())?;
    Ok((library, image - 1))
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    format!("{:x}", digest.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture(format: MapFormat, width: u16, height: u16) -> Vec<u8> {
        let length = expected_len(width, height).unwrap();
        let mut bytes = vec![0_u8; length];
        if format == MapFormat::Aragom31 {
            bytes[..20].copy_from_slice(b"Aragom\xB5\xD8\xCD\xBC\xB1\xE0\xBC\xAD\xC6\xF73.1\0");
        }
        write_u16(&mut bytes, 22, width);
        write_u16(&mut bytes, 24, height);
        let blocks = back_block_count(width, height);
        for index in 0..blocks {
            let offset = HEADER_LEN + index * BACK_BLOCK_LEN;
            bytes[offset] = 1;
            write_u16(&mut bytes, offset + 1, 41);
        }
        let start = HEADER_LEN + blocks * BACK_BLOCK_LEN;
        for index in 0..usize::from(width) * usize::from(height) {
            bytes[start + index * CELL_LEN] = 0x03;
            bytes[start + index * CELL_LEN + 3] = 0xff;
            bytes[start + index * CELL_LEN + 4] = 0xff;
        }
        bytes
    }

    #[test]
    fn detects_supported_headers_and_round_trips_without_changes() {
        for format in [MapFormat::Aragom31, MapFormat::Mir3ZeroHeader] {
            let bytes = fixture(format, 100, 80);
            let document = MapDocument::parse(bytes.clone()).unwrap();
            assert_eq!(document.header().format, format);
            assert_eq!(document.into_bytes(), bytes);
        }
    }

    #[test]
    fn map_edit_dto_uses_camel_case_fields() {
        for value in [
            serde_json::json!({"type":"setSprite","x":1,"y":2,"layer":"front","library":3,"image":4}),
            serde_json::json!({"type":"clearSprite","x":1,"y":2,"layer":"middle"}),
            serde_json::json!({"type":"setCollision","x":1,"y":2,"walkable":true,"frontBlocked":false}),
            serde_json::json!({"type":"setDoor","x":1,"y":2,"doorIndex":3,"doorOffset":4}),
            serde_json::json!({"type":"setAnimation","x":1,"y":2,"middleFrames":3,"frontFrames":4}),
        ] {
            let operation: MapEditOperation = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(operation).unwrap(), value);
        }
    }

    #[test]
    fn edits_only_bound_sprite_bytes() {
        let bytes = fixture(MapFormat::Mir3ZeroHeader, 4, 4);
        let mut document = MapDocument::parse(bytes.clone()).unwrap();
        document
            .apply(&[MapEditOperation::SetSprite {
                x: 2,
                y: 3,
                layer: MapLayer::Front,
                library: 9,
                image: 123,
            }])
            .unwrap();
        let updated = document.into_bytes();
        let changed: Vec<usize> = bytes
            .iter()
            .zip(&updated)
            .enumerate()
            .filter_map(|(index, (before, after))| (before != after).then_some(index))
            .collect();
        let offset = cell_offset(4, 4, 2, 3);
        assert_eq!(changed, vec![offset + 3, offset + 7]);
    }

    #[test]
    fn background_is_shared_by_two_by_two_block() {
        let bytes = fixture(MapFormat::Mir3ZeroHeader, 4, 4);
        let mut document = MapDocument::parse(bytes).unwrap();
        document
            .apply(&[MapEditOperation::SetSprite {
                x: 1,
                y: 1,
                layer: MapLayer::Background,
                library: 2,
                image: 77,
            }])
            .unwrap();
        assert_eq!(document.cell(0, 0).unwrap().background.image, 77);
        assert_eq!(document.cell(1, 1).unwrap().background.image, 77);
        assert_eq!(document.cell(2, 0).unwrap().background.image, 42);
    }

    #[test]
    fn creates_cleared_resized_map_from_supported_template() {
        let template = MapDocument::parse(fixture(MapFormat::Aragom31, 4, 4)).unwrap();
        let created = template
            .clone_cleared(NewMapOptions {
                width: 8,
                height: 6,
                background_library: 3,
                background_image: 15,
                walkable: false,
            })
            .unwrap();
        assert_eq!(created.header().width, 8);
        assert_eq!(created.header().height, 6);
        let cell = created.cell(7, 5).unwrap();
        assert_eq!(
            cell.background,
            MapSpriteRef {
                library: 3,
                image: 15
            }
        );
        assert!(!cell.walkable);
    }

    #[test]
    fn rejects_unknown_or_length_mismatched_layouts() {
        let mut bytes = fixture(MapFormat::Mir3ZeroHeader, 4, 4);
        bytes.pop();
        let document = MapDocument::parse(bytes).unwrap();
        assert_eq!(document.header().format, MapFormat::Unknown);
        assert!(!document.header().capabilities.terrain_editable);
    }

    #[test]
    fn detects_optional_external_corpus_without_panicking() {
        let Ok(root) = std::env::var("MIR3_MAP_CORPUS") else {
            return;
        };
        let mut detected = 0_usize;
        for entry in fs::read_dir(root).expect("地图语料目录应可读取") {
            let entry = entry.expect("地图语料目录项应可读取");
            if !entry
                .path()
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("map"))
            {
                continue;
            }
            let bytes = fs::read(entry.path()).expect("地图语料应可读取");
            let _ = MapDocument::parse(bytes).expect("任何地图都应稳定返回文档或只读诊断");
            detected += 1;
        }
        assert!(detected > 0, "地图语料目录应至少包含一个 .map 文件");
    }
}
