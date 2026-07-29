pub mod auth;
pub mod config;
pub mod contract;
pub mod identity;
pub mod pairing;
pub mod registry;

use std::net::SocketAddr;

use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde::Serialize;
use url::Url;
use uuid::Uuid;

pub const MOBILE_PROTOCOL_VERSION: u16 = 1;
pub const MAX_MOBILE_REQUEST_BYTES: usize = 64 * 1024;
pub const MAX_MOBILE_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
pub const MOBILE_REQUEST_WINDOW_SECONDS: i64 = 300;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MobileGatewayStatus {
    configured: bool,
    enabled: bool,
    bind: Option<String>,
    advertised_endpoint: Option<String>,
    device_count: usize,
    active_device_count: usize,
    revoked_device_count: usize,
}

pub fn run_enable(bind: SocketAddr, endpoint: &str) -> Result<()> {
    let config = config::MobileGatewayConfig {
        enabled: true,
        bind,
        advertised_endpoint: endpoint.to_owned(),
    };
    config::validate_mobile_config(&config)?;
    let endpoint = Url::parse(endpoint).context("mobile gateway endpoint must be a valid URL")?;
    let subject_alt_name = endpoint
        .host_str()
        .context("mobile gateway endpoint must contain a host")?;
    let coven_home = crate::coven_home_dir()?;
    identity::load_or_create_host_identity(&coven_home, subject_alt_name)?;
    config::save_mobile_config(&coven_home, &config)?;
    println!(
        "Mobile memory access enabled at {}",
        config.advertised_endpoint
    );
    println!("Restart the Coven daemon to apply this listener configuration.");
    Ok(())
}

pub fn run_disable(forget_devices: bool, confirm_forget_devices: bool) -> Result<()> {
    if forget_devices != confirm_forget_devices {
        bail!("forgetting devices requires both --forget-devices and --confirm-forget-devices");
    }
    let coven_home = crate::coven_home_dir()?;
    let was_configured = config::remove_mobile_config(&coven_home)?;
    if forget_devices {
        registry::DeviceRegistry::load(&coven_home)?.forget_all()?;
    }
    println!(
        "Mobile memory access {}.",
        if was_configured {
            "disabled"
        } else {
            "was already disabled"
        }
    );
    if forget_devices {
        println!("All paired mobile devices were forgotten.");
    } else {
        println!("Host identity and paired devices were retained.");
    }
    Ok(())
}

pub fn run_status(json: bool) -> Result<()> {
    let coven_home = crate::coven_home_dir()?;
    let config = config::load_mobile_config(&coven_home)?;
    let devices = registry::DeviceRegistry::load_if_present(&coven_home)?
        .map(|registry| registry.list_status())
        .transpose()?
        .unwrap_or_default();
    let active_device_count = devices
        .iter()
        .filter(|device| device.revoked_at.is_none())
        .count();
    let status = MobileGatewayStatus {
        configured: config.is_some(),
        enabled: config.as_ref().is_some_and(|config| config.enabled),
        bind: config.as_ref().map(|config| config.bind.to_string()),
        advertised_endpoint: config
            .as_ref()
            .map(|config| config.advertised_endpoint.clone()),
        device_count: devices.len(),
        active_device_count,
        revoked_device_count: devices.len() - active_device_count,
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&status)?);
    } else {
        println!(
            "Mobile memory access: {}",
            if status.enabled {
                "enabled"
            } else {
                "disabled"
            }
        );
        if let Some(endpoint) = status.advertised_endpoint {
            println!("Endpoint: {endpoint}");
        }
        println!(
            "Devices: {} active, {} revoked",
            status.active_device_count, status.revoked_device_count
        );
    }
    Ok(())
}

pub fn run_devices(json: bool) -> Result<()> {
    let coven_home = crate::coven_home_dir()?;
    let devices = registry::DeviceRegistry::load_if_present(&coven_home)?
        .map(|registry| registry.list_status())
        .transpose()?
        .unwrap_or_default();
    if json {
        println!("{}", serde_json::to_string_pretty(&devices)?);
    } else if devices.is_empty() {
        println!("No mobile devices are paired.");
    } else {
        for device in devices {
            let state = if device.revoked_at.is_some() {
                "revoked"
            } else {
                "active"
            };
            println!("{}\t{}\t{}", device.id, state, device.display_name);
        }
    }
    Ok(())
}

pub fn run_revoke_device(device_id: Uuid) -> Result<()> {
    let coven_home = crate::coven_home_dir()?;
    let registry = registry::DeviceRegistry::load_if_present(&coven_home)?
        .context("no mobile devices are paired")?;
    registry.revoke(device_id, Utc::now())?;
    println!("Revoked mobile device {device_id}.");
    Ok(())
}

pub fn run_pair() -> Result<()> {
    bail!("mobile pairing is not available until the mobile gateway is running")
}
