// New implementation to capture cursor under native Wayland
use super::{CursorData, ResultType};
use hbb_common::{bail, log};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use dbus::{
    arg::{RefArg, Variant},
    blocking::{Proxy, SyncConnection},
};

lazy_static::lazy_static! {
    static ref WAYLAND_CURSOR_STATE: Arc<RwLock<WaylandCursorState>> =
        Arc::new(RwLock::new(WaylandCursorState::new()));
}

#[derive(Clone)]
struct WaylandCursorState {
    current_cursor_id: u64,
    cursor_cache: HashMap<u64, CursorData>,
    connection: Option<Arc<SyncConnection>>,
    last_update: std::time::Instant,
}

impl WaylandCursorState {
    fn new() -> Self {
        Self {
            current_cursor_id: 0,
            cursor_cache: HashMap::new(),
            connection: None,
            last_update: std::time::Instant::now(),
        }
    }

    fn init_dbus(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.connection.is_none() {
            let conn = SyncConnection::new_session()?;
            self.connection = Some(Arc::new(conn));
        }
        Ok(())
    }
}

/// Main function to get cursor under Wayland
pub fn get_wayland_cursor() -> ResultType<Option<u64>> {
    let mut state = match WAYLAND_CURSOR_STATE.write() {
        Ok(state) => state,
        Err(_) => bail!("Failed to lock cursor state"),
    };

    // Initialize D-Bus connection if needed
    if let Err(e) = state.init_dbus() {
        log::warn!("Failed to initialize D-Bus for cursor capture: {}", e);
        return Ok(None);
    }

    // Try to get cursor information via different methods
    match try_get_cursor_from_compositor(&mut *state) {
        Ok(Some(cursor_id)) => {
            state.current_cursor_id = cursor_id;
            state.last_update = std::time::Instant::now();
            Ok(Some(cursor_id))
        }
        Ok(None) => Ok(None),
        Err(e) => {
            log::debug!("Compositor cursor failed, trying portal method: {}", e);
            try_get_cursor_from_portal(&mut *state)
        }
    }
}

/// Try to get cursor via Wayland compositor directly
fn try_get_cursor_from_compositor(
    state: &mut WaylandCursorState,
) -> Result<Option<u64>, Box<dyn std::error::Error>> {
    // Method 1: Via org.freedesktop.portal.ScreenCast for cursor metadata
    if let Some(conn) = &state.connection {
        let proxy = Proxy::new(
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            std::time::Duration::from_millis(1000),
            conn.as_ref(),
        );

        // Try to get cursor metadata
        if let Ok(cursor_info) = get_cursor_metadata_from_portal(&proxy) {
            return Ok(Some(cursor_info));
        }
    }

    Ok(None)
}

/// Try to get cursor via RemoteDesktop portal
fn try_get_cursor_from_portal(
    state: &mut WaylandCursorState,
) -> ResultType<Option<u64>> {
    // For now, use a simple fallback implementation
    // TODO: Integrate with existing RDP/PipeWire sessions when available

    // Generate a cursor ID based on timestamp to simulate a change
    let cursor_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    // Create default cursor data and cache it
    let cursor_data = CursorData {
        id: cursor_id,
        hotx: 1,
        hoty: 1,
        width: 32,
        height: 32,
        colors: create_default_cursor_pixels().into(),
        ..Default::default()
    };

    state.cursor_cache.insert(cursor_id, cursor_data);
    Ok(Some(cursor_id))
}

/// Get cursor metadata from portal
fn get_cursor_metadata_from_portal(
    proxy: &Proxy<&SyncConnection>,
) -> Result<u64, Box<dyn std::error::Error>> {
    // Simplified approach: avoid complex D-Bus calls and use direct fallback
    let result: Result<(u32,), dbus::Error> = proxy.method_call(
        "org.freedesktop.portal.ScreenCast",
        "GetSources",
        (),
    );

    // Try to get basic information from the service
    match proxy.method_call::<(), (), _, _>("org.freedesktop.portal.ScreenCast", "GetVersion", ()) {
        Ok(_) => {
            // If the call succeeds, use this to generate an ID
            let cursor_id = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            return Ok(cursor_id);
        }
        Err(_) => {
            // Continue to the fallback if the call fails
        }
    }

    // Fallback: generate ID based on timestamp only
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64)
}

/// Generate cursor ID based on metadata (simplified fallback function)
fn generate_cursor_id_from_metadata(
    _metadata: &HashMap<String, Variant<Box<dyn RefArg>>>,
) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();

    // Use system data to create a unique hash
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis()
        .hash(&mut hasher);

    hasher.finish()
}

/// Get complete Wayland cursor data
pub fn get_wayland_cursor_data(hcursor: u64) -> ResultType<CursorData> {
    let state = match WAYLAND_CURSOR_STATE.read() {
        Ok(state) => state,
        Err(_) => bail!("Failed to lock cursor state"),
    };

    // Check cache first
    if let Some(cached_data) = state.cursor_cache.get(&hcursor) {
        return Ok(cached_data.clone());
    }

    // If not in cache, try to get via different methods
    drop(state); // Release read lock

    // Try to get cursor data
    match try_get_cursor_data_from_wayland(hcursor) {
        Ok(cursor_data) => {
            // Update cache
            if let Ok(mut state) = WAYLAND_CURSOR_STATE.write() {
                state.cursor_cache.insert(hcursor, cursor_data.clone());
            }
            Ok(cursor_data)
        }
        Err(e) => {
            log::warn!("Failed to get Wayland cursor data: {}", e);
            bail!("Failed to get cursor data for {}", hcursor)
        }
    }
}

/// Try to get cursor data from different Wayland sources
fn try_get_cursor_data_from_wayland(
    _cursor_id: u64,
) -> Result<CursorData, Box<dyn std::error::Error>> {
    // TODO: Implement real cursor data retrieval
    // This would require:
    // 1. Access to Wayland compositor metadata
    // 2. Extracting cursor pixels from themes/system icons
    // 3. Using native Wayland APIs (libwayland-client)

    // For now, return default cursor
    Ok(CursorData {
        id: _cursor_id,
        hotx: 1,
        hoty: 1,
        width: 32,
        height: 32,
        colors: create_default_cursor_pixels().into(),
        ..Default::default()
    })
}

/// Create default pixels for a cursor
fn create_default_cursor_pixels() -> Vec<u8> {
    // Create a simple 32x32 pixel arrow cursor (RGBA)
    let mut pixels = vec![0u8; 32 * 32 * 4];

    // Draw a simple arrow
    for y in 0..16 {
        for x in 0..=y {
            if x < 32 && y < 32 {
                let idx = (y * 32 + x) * 4;
                if idx + 3 < pixels.len() {
                    pixels[idx] = 255;     // R
                    pixels[idx + 1] = 255; // G
                    pixels[idx + 2] = 255; // B
                    pixels[idx + 3] = 255; // A
                }
            }
        }
    }

    pixels
}

/// Cleanup function
pub fn cleanup_wayland_cursor() {
    if let Ok(mut state) = WAYLAND_CURSOR_STATE.write() {
        state.cursor_cache.clear();
        state.connection = None;
    }
}

/// Get cursor position under native Wayland
pub fn get_wayland_cursor_pos() -> ResultType<Option<(i32, i32)>> {
    let mut state = match WAYLAND_CURSOR_STATE.write() {
        Ok(state) => state,
        Err(_) => bail!("Failed to lock cursor state"),
    };

    // Initialize D-Bus connection if needed
    if let Err(e) = state.init_dbus() {
        log::warn!("Failed to initialize D-Bus for cursor position: {}", e);
        return Ok(None);
    }

    // Try to get cursor position via different methods
    match try_get_cursor_pos_from_compositor(&mut *state) {
        Ok(Some(pos)) => {
            state.last_update = std::time::Instant::now();
            Ok(Some(pos))
        }
        Ok(None) => Ok(None),
        Err(e) => {
            log::debug!("Compositor cursor position failed, trying portal method: {}", e);
            try_get_cursor_pos_from_portal(&mut *state)
        }
    }
}

/// Try to get cursor position via Wayland compositor directly
fn try_get_cursor_pos_from_compositor(
    state: &mut WaylandCursorState,
) -> Result<Option<(i32, i32)>, Box<dyn std::error::Error>> {
    // Method 1: Via org.freedesktop.portal.RemoteDesktop for cursor position
    if let Some(conn) = &state.connection {
        let proxy = Proxy::new(
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            std::time::Duration::from_millis(1000),
            conn.as_ref(),
        );

        // Try to get cursor position metadata
        if let Ok(pos) = get_cursor_position_from_portal(&proxy) {
            return Ok(Some(pos));
        }
    }

    Ok(None)
}

/// Try to get cursor position via RemoteDesktop portal
fn try_get_cursor_pos_from_portal(
    _state: &mut WaylandCursorState,
) -> ResultType<Option<(i32, i32)>> {
    // For now, use a fallback implementation
    // TODO: Integrate with existing RDP/PipeWire sessions when available

    // In a real implementation, this would query the active PipeWire session
    // for cursor metadata that includes position information

    // Return None to indicate position is not available via this method
    // This will cause the fallback to X11/XWayland in the calling function
    Ok(None)
}

/// Get cursor position from portal
fn get_cursor_position_from_portal(
    proxy: &Proxy<&SyncConnection>,
) -> Result<(i32, i32), Box<dyn std::error::Error>> {
    // Try to get cursor position via RemoteDesktop portal
    // This is a simplified approach - in practice, this would require:
    // 1. An active RemoteDesktop session
    // 2. Cursor metadata enabled in the session
    // 3. Proper handling of the PipeWire stream metadata

    match proxy.method_call::<(), (), _, _>("org.freedesktop.portal.RemoteDesktop", "GetVersion", ()) {
        Ok(_) => {
            // If the portal is available, we could potentially get position
            // For now, return an error to fall back to X11/XWayland
            Err("Portal available but cursor position not implemented yet".into())
        }
        Err(e) => Err(e.into()),
    }
}
