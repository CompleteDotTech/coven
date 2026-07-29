use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::config::{atomic_replace_private, ensure_private_mobile_dir, validate_private_file};
use super::contract::{MobileDeviceScope, MobilePairedDevice};

pub const DEVICES_FILE: &str = "devices.json";
const DEVICE_REGISTRY_VERSION: u16 = 1;
const MAX_DEVICE_RECORDS: usize = 128;
const MAX_DEVICE_NAME_CHARS: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeviceRecord {
    pub id: Uuid,
    pub display_name: String,
    pub public_key_x963: String,
    pub paired_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub scopes: Vec<DeviceScope>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceScope {
    MemoryRead,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct StoredDeviceRegistry {
    version: u16,
    devices: Vec<DeviceRecord>,
}

pub struct DeviceRegistry {
    path: PathBuf,
    devices: RwLock<Vec<DeviceRecord>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStatusRecord {
    pub id: Uuid,
    pub display_name: String,
    pub paired_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub scopes: Vec<DeviceScope>,
}

impl DeviceRegistry {
    pub fn load(coven_home: &Path) -> Result<Self> {
        let mobile_dir = ensure_private_mobile_dir(coven_home)?;
        let path = mobile_dir.join(DEVICES_FILE);
        let devices = read_registry(&path)?;
        Ok(Self {
            path,
            devices: RwLock::new(devices),
        })
    }

    pub fn load_if_present(coven_home: &Path) -> Result<Option<Self>> {
        match fs::symlink_metadata(coven_home.join(super::config::MOBILE_STATE_DIR)) {
            Ok(_) => Self::load(coven_home).map(Some),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error).context("failed to inspect mobile state directory"),
        }
    }

    pub fn reload(&self) -> Result<()> {
        let devices = read_registry(&self.path)?;
        *self
            .devices
            .write()
            .map_err(|_| anyhow::anyhow!("mobile device registry lock poisoned"))? = devices;
        Ok(())
    }

    pub fn register(&self, record: DeviceRecord) -> Result<()> {
        validate_device(&record)?;
        let mut devices = self
            .devices
            .write()
            .map_err(|_| anyhow::anyhow!("mobile device registry lock poisoned"))?;
        if devices.len() >= MAX_DEVICE_RECORDS {
            bail!("mobile device registry is full");
        }
        if devices.iter().any(|existing| existing.id == record.id) {
            bail!("mobile device id is already registered");
        }
        if devices
            .iter()
            .any(|existing| existing.public_key_x963 == record.public_key_x963)
        {
            bail!("mobile device public key is already registered");
        }
        let mut updated = devices.clone();
        updated.push(record);
        validate_devices(&updated)?;
        write_registry(&self.path, &updated)?;
        *devices = updated;
        Ok(())
    }

    pub fn revoke(&self, device_id: Uuid, revoked_at: DateTime<Utc>) -> Result<()> {
        let mut devices = self
            .devices
            .write()
            .map_err(|_| anyhow::anyhow!("mobile device registry lock poisoned"))?;
        let mut updated = devices.clone();
        let device = updated
            .iter_mut()
            .find(|record| record.id == device_id)
            .context("mobile device is not registered")?;
        if device.revoked_at.is_none() {
            device.revoked_at = Some(revoked_at);
        }
        write_registry(&self.path, &updated)?;
        *devices = updated;
        Ok(())
    }

    pub fn forget_all(&self) -> Result<()> {
        let mut devices = self
            .devices
            .write()
            .map_err(|_| anyhow::anyhow!("mobile device registry lock poisoned"))?;
        write_registry(&self.path, &[])?;
        devices.clear();
        Ok(())
    }

    pub fn device(&self, device_id: Uuid) -> Result<Option<DeviceRecord>> {
        Ok(self
            .devices
            .read()
            .map_err(|_| anyhow::anyhow!("mobile device registry lock poisoned"))?
            .iter()
            .find(|record| record.id == device_id)
            .cloned())
    }

    pub fn active_device(&self, device_id: Uuid) -> Result<Option<DeviceRecord>> {
        Ok(self
            .device(device_id)?
            .filter(|record| record.revoked_at.is_none()))
    }

    pub fn list_redacted(&self) -> Result<Vec<MobilePairedDevice>> {
        Ok(self
            .devices
            .read()
            .map_err(|_| anyhow::anyhow!("mobile device registry lock poisoned"))?
            .iter()
            .map(|record| MobilePairedDevice {
                id: record.id,
                display_name: record.display_name.clone(),
                paired_at: record.paired_at,
                scopes: record
                    .scopes
                    .iter()
                    .map(|scope| match scope {
                        DeviceScope::MemoryRead => MobileDeviceScope::MemoryRead,
                    })
                    .collect(),
            })
            .collect())
    }

    pub fn list_status(&self) -> Result<Vec<DeviceStatusRecord>> {
        Ok(self
            .devices
            .read()
            .map_err(|_| anyhow::anyhow!("mobile device registry lock poisoned"))?
            .iter()
            .map(|record| DeviceStatusRecord {
                id: record.id,
                display_name: record.display_name.clone(),
                paired_at: record.paired_at,
                revoked_at: record.revoked_at,
                scopes: record.scopes.clone(),
            })
            .collect())
    }
}

fn read_registry(path: &Path) -> Result<Vec<DeviceRecord>> {
    match fs::symlink_metadata(path) {
        Ok(_) => validate_private_file(path)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to inspect {}", path.display()));
        }
    }
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let stored: StoredDeviceRegistry = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    if stored.version != DEVICE_REGISTRY_VERSION {
        bail!("unsupported mobile device registry version");
    }
    validate_devices(&stored.devices)?;
    Ok(stored.devices)
}

fn write_registry(path: &Path, devices: &[DeviceRecord]) -> Result<()> {
    validate_devices(devices)?;
    let stored = StoredDeviceRegistry {
        version: DEVICE_REGISTRY_VERSION,
        devices: devices.to_vec(),
    };
    let mut encoded =
        serde_json::to_vec_pretty(&stored).context("failed to encode mobile device registry")?;
    encoded.push(b'\n');
    atomic_replace_private(path, &encoded)
}

fn validate_devices(devices: &[DeviceRecord]) -> Result<()> {
    if devices.len() > MAX_DEVICE_RECORDS {
        bail!("mobile device registry exceeds the record limit");
    }
    for (index, device) in devices.iter().enumerate() {
        validate_device(device)?;
        if devices[..index]
            .iter()
            .any(|existing| existing.id == device.id)
        {
            bail!("mobile device registry contains duplicate ids");
        }
        if devices[..index]
            .iter()
            .any(|existing| existing.public_key_x963 == device.public_key_x963)
        {
            bail!("mobile device registry contains duplicate public keys");
        }
    }
    Ok(())
}

fn validate_device(device: &DeviceRecord) -> Result<()> {
    let name = device.display_name.as_str();
    let name_chars = name.chars().count();
    if name.is_empty()
        || name.trim() != name
        || name_chars > MAX_DEVICE_NAME_CHARS
        || name.chars().any(char::is_control)
    {
        bail!("mobile device display name is invalid");
    }
    let public_key = URL_SAFE_NO_PAD
        .decode(&device.public_key_x963)
        .context("mobile device public key is not valid base64url")?;
    if public_key.len() != 65
        || public_key.first() != Some(&4)
        || URL_SAFE_NO_PAD.encode(&public_key) != device.public_key_x963
        || p256::PublicKey::from_sec1_bytes(&public_key).is_err()
    {
        bail!("mobile device public key is not a canonical P-256 X9.63 key");
    }
    if device.scopes != [DeviceScope::MemoryRead] {
        bail!("mobile device must have exactly the memory_read scope");
    }
    if device
        .revoked_at
        .is_some_and(|revoked_at| revoked_at < device.paired_at)
    {
        bail!("mobile device revocation cannot predate pairing");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use p256::elliptic_curve::sec1::ToEncodedPoint;

    fn device(id: Uuid, name: &str) -> DeviceRecord {
        let mut scalar = [1_u8; 32];
        for (target, source) in scalar[16..].iter_mut().zip(id.as_bytes()) {
            *target ^= source;
        }
        let secret = p256::SecretKey::from_slice(&scalar).unwrap();
        DeviceRecord {
            id,
            display_name: name.to_owned(),
            public_key_x963: URL_SAFE_NO_PAD
                .encode(secret.public_key().to_encoded_point(false).as_bytes()),
            paired_at: Utc::now(),
            revoked_at: None,
            scopes: vec![DeviceScope::MemoryRead],
        }
    }

    #[test]
    fn revoked_device_never_authenticates_after_atomic_reload() {
        let temp = tempfile::tempdir().unwrap();
        let first = DeviceRegistry::load(temp.path()).unwrap();
        let record = device(Uuid::new_v4(), "Synthetic phone");
        first.register(record.clone()).unwrap();

        let second = DeviceRegistry::load(temp.path()).unwrap();
        second.revoke(record.id, Utc::now()).unwrap();
        first.reload().unwrap();

        assert!(first.active_device(record.id).unwrap().is_none());
        assert!(first
            .device(record.id)
            .unwrap()
            .unwrap()
            .revoked_at
            .is_some());
    }

    #[test]
    fn registry_rejects_duplicate_public_keys_and_ids() {
        let temp = tempfile::tempdir().unwrap();
        let registry = DeviceRegistry::load(temp.path()).unwrap();
        let first = device(Uuid::new_v4(), "Synthetic phone");
        registry.register(first.clone()).unwrap();

        let mut duplicate_id = device(first.id, "Other synthetic phone");
        assert!(registry.register(duplicate_id.clone()).is_err());
        duplicate_id.id = Uuid::new_v4();
        duplicate_id.public_key_x963 = first.public_key_x963;
        assert!(registry.register(duplicate_id).is_err());
    }

    #[test]
    fn registry_corruption_fails_closed_without_overwrite() {
        let temp = tempfile::tempdir().unwrap();
        let mobile = ensure_private_mobile_dir(temp.path()).unwrap();
        let path = mobile.join(DEVICES_FILE);
        atomic_replace_private(&path, b"{not valid json}\n").unwrap();
        let before = std::fs::read(&path).unwrap();

        assert!(DeviceRegistry::load(temp.path()).is_err());
        assert_eq!(std::fs::read(path).unwrap(), before);
    }

    #[test]
    fn device_status_output_omits_public_keys() {
        let temp = tempfile::tempdir().unwrap();
        let registry = DeviceRegistry::load(temp.path()).unwrap();
        let record = device(Uuid::new_v4(), "Synthetic phone");
        registry.register(record.clone()).unwrap();

        let encoded = serde_json::to_value(registry.list_status().unwrap()).unwrap();
        assert_eq!(encoded[0]["id"], record.id.to_string());
        assert!(encoded[0].get("publicKeyX963").is_none());
        assert!(encoded[0].get("publicKey").is_none());
    }
}
