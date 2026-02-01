// LXMonitor - Universal ArtNet/sACN Monitor
// Main Tauri application entry point

mod network;

use network::{
    create_source_manager, start_artnet_listener, start_sacn_listener, 
    start_status_updater, DmxStore, DmxStoreHandle, ListenerEvent,
    NetworkSource, SourceManagerHandle,
};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::broadcast;

/// Application state
pub struct AppState {
    source_manager: SourceManagerHandle,
    dmx_store: DmxStoreHandle,
    event_tx: broadcast::Sender<ListenerEvent>,
    is_listening: Mutex<bool>,
}

/// Get all discovered sources
#[tauri::command]
async fn get_sources(state: State<'_, AppState>) -> Result<Vec<NetworkSource>, String> {
    Ok(state.source_manager.get_all_sources())
}

/// Get DMX data for a specific universe
#[tauri::command]
async fn get_dmx_data(state: State<'_, AppState>, universe: u16) -> Result<Option<Vec<u8>>, String> {
    Ok(state.dmx_store.get(universe))
}

/// Get DMX data for all universes
#[tauri::command]
async fn get_all_dmx_data(state: State<'_, AppState>) -> Result<std::collections::HashMap<u16, Vec<u8>>, String> {
    Ok(state.dmx_store.get_all())
}

/// Network interface info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterface {
    pub name: String,
    pub ip: String,
    pub is_loopback: bool,
}

/// Get available network interfaces
#[tauri::command]
async fn get_network_interfaces() -> Result<Vec<NetworkInterface>, String> {
    let mut interfaces = Vec::new();
    
    // Add "all interfaces" option
    interfaces.push(NetworkInterface {
        name: "All Interfaces".to_string(),
        ip: "0.0.0.0".to_string(),
        is_loopback: false,
    });
    
    // Get local interfaces
    if let Ok(local_ip) = local_ip_address::local_ip() {
        interfaces.push(NetworkInterface {
            name: format!("Primary ({})", local_ip),
            ip: local_ip.to_string(),
            is_loopback: false,
        });
    }
    
    // Try to get all interfaces
    if let Ok(ifaces) = local_ip_address::list_afinet_netifas() {
        for (name, ip) in ifaces {
            if let std::net::IpAddr::V4(ipv4) = ip {
                if ipv4 != Ipv4Addr::LOCALHOST && !interfaces.iter().any(|i| i.ip == ipv4.to_string()) {
                    interfaces.push(NetworkInterface {
                        name,
                        ip: ipv4.to_string(),
                        is_loopback: ipv4.is_loopback(),
                    });
                }
            }
        }
    }
    
    Ok(interfaces)
}

/// Listener status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListenerStatus {
    pub is_listening: bool,
    pub artnet_active: bool,
    pub sacn_active: bool,
}

/// Get listener status
#[tauri::command]
async fn get_listener_status(state: State<'_, AppState>) -> Result<ListenerStatus, String> {
    let is_listening = *state.is_listening.lock();
    Ok(ListenerStatus {
        is_listening,
        artnet_active: is_listening,
        sacn_active: is_listening,
    })
}

/// Start the network event forwarder to send events to the frontend
fn start_event_forwarder(app_handle: AppHandle, mut event_rx: broadcast::Receiver<ListenerEvent>, state: AppState) {
    let source_manager = state.source_manager.clone();
    let dmx_store = state.dmx_store.clone();
    
    tauri::async_runtime::spawn(async move {
        loop {
            match event_rx.recv().await {
                Ok(event) => {
                    match event {
                        ListenerEvent::SourcesUpdated => {
                            let sources = source_manager.get_all_sources();
                            let _ = app_handle.emit("sources-updated", sources);
                        }
                        ListenerEvent::DmxData(data) => {
                            // Emit DMX data for the specific universe
                            let _ = app_handle.emit(&format!("dmx-{}", data.universe), &data.data);
                            // Also emit a general DMX update event
                            let _ = app_handle.emit("dmx-updated", serde_json::json!({
                                "universe": data.universe,
                                "sourceIp": data.source_ip.to_string(),
                                "timestamp": data.timestamp
                            }));
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    eprintln!("Event forwarder lagged {} messages", n);
                }
                Err(broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    });
}

/// Start the network listeners
fn start_listeners(source_manager: SourceManagerHandle, dmx_store: DmxStoreHandle, event_tx: broadcast::Sender<ListenerEvent>) {
    let bind_addr = Ipv4Addr::UNSPECIFIED;
    
    // Start Art-Net listener
    let sm = source_manager.clone();
    let ds = dmx_store.clone();
    let tx = event_tx.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = start_artnet_listener(sm, ds, tx, bind_addr).await {
            eprintln!("[Art-Net] Listener error: {}", e);
        }
    });
    
    // Start sACN listener
    let sm = source_manager.clone();
    let ds = dmx_store.clone();
    let tx = event_tx.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = start_sacn_listener(sm, ds, tx, bind_addr).await {
            eprintln!("[sACN] Listener error: {}", e);
        }
    });
    
    // Start status updater
    let sm = source_manager.clone();
    let tx = event_tx.clone();
    tauri::async_runtime::spawn(async move {
        start_status_updater(sm, tx).await;
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Create shared state
    let source_manager = create_source_manager();
    let dmx_store = Arc::new(DmxStore::new());
    let (event_tx, _) = broadcast::channel::<ListenerEvent>(1000);
    
    let app_state = AppState {
        source_manager: source_manager.clone(),
        dmx_store: dmx_store.clone(),
        event_tx: event_tx.clone(),
        is_listening: Mutex::new(true),
    };
    
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            get_sources,
            get_dmx_data,
            get_all_dmx_data,
            get_network_interfaces,
            get_listener_status,
        ])
        .setup(move |app| {
            let app_handle = app.handle().clone();
            let event_rx = event_tx.subscribe();
            
            // Create state for event forwarder
            let forwarder_state = AppState {
                source_manager: source_manager.clone(),
                dmx_store: dmx_store.clone(),
                event_tx: event_tx.clone(),
                is_listening: Mutex::new(true),
            };
            
            // Start event forwarder
            start_event_forwarder(app_handle, event_rx, forwarder_state);
            
            // Start network listeners
            start_listeners(source_manager, dmx_store, event_tx);
            
            println!("LXMonitor started - listening for Art-Net and sACN traffic");
            
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
