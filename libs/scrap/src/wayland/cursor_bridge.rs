// Bridge for Wayland cursor management via PipeWire
use std::sync::{Arc, Mutex, RwLock};
use std::collections::HashMap;

/// Structure to store Wayland cursor metadata
#[derive(Debug, Clone, Default)]
pub struct WaylandCursorData {
    pub id: u64,
    pub hotx: i32,
    pub hoty: i32,
    pub width: u32,
    pub height: u32,
    pub colors: Vec<u8>, // RGBA format
    pub position: (i32, i32),
    pub visible: bool,
}

/// Global bridge to share cursor data between PipeWire and get_cursor* functions
pub struct CursorBridge {
    current_cursor: Arc<RwLock<Option<WaylandCursorData>>>,
    cursor_position: Arc<RwLock<(i32, i32)>>,
    cursor_cache: Arc<Mutex<HashMap<u64, WaylandCursorData>>>,
}

impl CursorBridge {
    fn new() -> Self {
        Self {
            current_cursor: Arc::new(RwLock::new(None)),
            cursor_position: Arc::new(RwLock::new((0, 0))),
            cursor_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Updates cursor metadata from PipeWire
    pub fn update_cursor_metadata(&self, cursor_data: WaylandCursorData) {
        // Update position
        *self.cursor_position.write().unwrap() = cursor_data.position;

        // Cache cursor
        {
            let mut cache = self.cursor_cache.lock().unwrap();
            cache.insert(cursor_data.id, cursor_data.clone());
        }

        // Current cursor
        *self.current_cursor.write().unwrap() = Some(cursor_data);
    }

    /// Updates only cursor position
    pub fn update_cursor_position(&self, x: i32, y: i32) {
        *self.cursor_position.write().unwrap() = (x, y);

        // Also update position in current cursor if available
        if let Some(ref mut cursor) = self.current_cursor.write().unwrap().as_mut() {
            cursor.position = (x, y);
        }
    }

    /// Gets current cursor position
    pub fn get_cursor_position(&self) -> Option<(i32, i32)> {
        Some(*self.cursor_position.read().unwrap())
    }

    /// Gets current cursor ID
    pub fn get_current_cursor_id(&self) -> Option<u64> {
        self.current_cursor.read().unwrap().as_ref().map(|c| c.id)
    }

    /// Gets cursor data by ID
    pub fn get_cursor_data(&self, cursor_id: u64) -> Option<WaylandCursorData> {
        self.cursor_cache.lock().unwrap().get(&cursor_id).cloned()
    }

    /// Checks if cursor is visible
    pub fn is_cursor_visible(&self) -> bool {
        self.current_cursor.read().unwrap()
            .as_ref()
            .map(|c| c.visible)
            .unwrap_or(false)
    }
}

lazy_static::lazy_static! {
    /// Global instance of Wayland cursor bridge
    pub static ref WAYLAND_CURSOR_BRIDGE: CursorBridge = CursorBridge::new();
}

/// Utility functions for PipeWire integration

/// Converts raw cursor data from PipeWire to our format
pub fn convert_pipewire_cursor_data(
    id: u64,
    hotx: i32,
    hoty: i32,
    width: u32,
    height: u32,
    pixels: &[u32], // ARGB format from PipeWire
    position: (i32, i32),
    visible: bool,
) -> WaylandCursorData {
    let mut colors = Vec::with_capacity((width * height * 4) as usize);

    // Convert ARGB to RGBA
    for &pixel in pixels {
        let a = ((pixel >> 24) & 0xff) as u8;
        let r = ((pixel >> 16) & 0xff) as u8;
        let g = ((pixel >> 8) & 0xff) as u8;
        let b = (pixel & 0xff) as u8;

        colors.push(r);
        colors.push(g);
        colors.push(b);
        colors.push(a);
    }

    WaylandCursorData {
        id,
        hotx,
        hoty,
        width,
        height,
        colors,
        position,
        visible,
    }
}

/// Notifies the bridge of a cursor update from PipeWire
pub fn notify_cursor_update(cursor_data: WaylandCursorData) {
    WAYLAND_CURSOR_BRIDGE.update_cursor_metadata(cursor_data);
}

/// Notifies the bridge of a cursor position change
pub fn notify_cursor_position_update(x: i32, y: i32) {
    WAYLAND_CURSOR_BRIDGE.update_cursor_position(x, y);
}
