use std::sync::Arc;
use hbb_common::log;
use hbb_common::tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use hbb_common::message_proto::CursorData;

/// Cursor metadata extracted from video/pipewire stream
#[derive(Debug, Clone)]
pub struct CursorMetadata {
    pub position: Option<(i32, i32)>,
    pub visible: bool,
    pub shape_data: Option<Vec<u8>>,
    pub hotspot: Option<(i32, i32)>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub timestamp: u64,
}

impl Default for CursorMetadata {
    fn default() -> Self {
        Self {
            position: None,
            visible: true,
            shape_data: None,
            hotspot: None,
            width: None,
            height: None,
            timestamp: 0,
        }
    }
}

/// Trait for components that listen to cursor events
pub trait CursorEventSink: Send + Sync {
    fn on_cursor_metadata_update(&self, metadata: &CursorMetadata);
}

/// Centralized cursor event manager
pub struct CursorEventManager {
    sender: UnboundedSender<CursorMetadata>,
    receiver: Arc<hbb_common::tokio::sync::Mutex<UnboundedReceiver<CursorMetadata>>>,
    sinks: Arc<std::sync::RwLock<Vec<Arc<dyn CursorEventSink>>>>,
}

impl CursorEventManager {
    pub fn new() -> Self {
        let (sender, receiver) = unbounded_channel();
        Self {
            sender,
            receiver: Arc::new(hbb_common::tokio::sync::Mutex::new(receiver)),
            sinks: Arc::new(std::sync::RwLock::new(Vec::new())),
        }
    }

    /// Register a new sink to listen to cursor events
    pub fn register_sink(&self, sink: Arc<dyn CursorEventSink>) {
        let mut sinks = self.sinks.write().unwrap();
        sinks.push(sink);
    }

    /// Remove a sink from the listening list
    pub fn unregister_sink(&self, sink: Arc<dyn CursorEventSink>) {
        let mut sinks = self.sinks.write().unwrap();
        sinks.retain(|s| !Arc::ptr_eq(s, &sink));
    }

    /// Publish a cursor metadata update
    pub fn publish_cursor_metadata(&self, metadata: CursorMetadata) {
        if let Err(e) = self.sender.send(metadata) {
            log::warn!("Failed to send cursor metadata: {}", e);
        }
    }

    /// Start the event listening loop
    pub async fn start_event_loop(&self) {
        let receiver = self.receiver.clone();
        let sinks = self.sinks.clone();

        hbb_common::tokio::spawn(async move {
            let mut rx = receiver.lock().await;
            while let Some(metadata) = rx.recv().await {
                let sinks_guard = sinks.read().unwrap();
                for sink in sinks_guard.iter() {
                    sink.on_cursor_metadata_update(&metadata);
                }
            }
        });
    }

    /// Return a clone of the sender to publish events
    pub fn get_publisher(&self) -> UnboundedSender<CursorMetadata> {
        self.sender.clone()
    }
}

impl Default for CursorEventManager {
    fn default() -> Self {
        Self::new()
    }
}

lazy_static::lazy_static! {
    /// Global instance of the cursor event manager
    pub static ref CURSOR_EVENT_MANAGER: CursorEventManager = CursorEventManager::new();
}

/// Utility function to publish cursor metadata
pub fn publish_cursor_metadata(metadata: CursorMetadata) {
    CURSOR_EVENT_MANAGER.publish_cursor_metadata(metadata);
}

/// Utility function to register a sink
pub fn register_cursor_sink(sink: Arc<dyn CursorEventSink>) {
    CURSOR_EVENT_MANAGER.register_sink(sink);
}

/// Utility function to remove a sink
pub fn unregister_cursor_sink(sink: Arc<dyn CursorEventSink>) {
    CURSOR_EVENT_MANAGER.unregister_sink(sink);
}
