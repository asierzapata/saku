use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum DeviceIdError {
    #[error("Failed to determine saku data directory")]
    NoDataDir,

    #[error("Failed to create saku data directory '{path}': {source}")]
    CreateDirFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to read device_id from '{path}': {source}")]
    ReadFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to write device_id to '{path}': {source}")]
    WriteFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Returns the saku data directory: `~/.local/share/saku/`
pub fn saku_data_dir() -> Result<PathBuf, DeviceIdError> {
    dirs::data_local_dir()
        .map(|d| d.join("saku"))
        .ok_or(DeviceIdError::NoDataDir)
}

/// Reads the device_id from `~/.local/share/saku/device_id`, creating a new UUID v4 on first run.
pub fn get_or_create_device_id() -> Result<String, DeviceIdError> {
    let data_dir = saku_data_dir()?;
    let device_id_path = data_dir.join("device_id");

    match std::fs::read_to_string(&device_id_path) {
        Ok(content) => {
            let id = content.trim().to_string();
            if id.is_empty() {
                create_device_id(&data_dir, &device_id_path)
            } else {
                Ok(id)
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            create_device_id(&data_dir, &device_id_path)
        }
        Err(e) => Err(DeviceIdError::ReadFailed {
            path: device_id_path,
            source: e,
        }),
    }
}

fn create_device_id(data_dir: &PathBuf, device_id_path: &PathBuf) -> Result<String, DeviceIdError> {
    std::fs::create_dir_all(data_dir).map_err(|e| DeviceIdError::CreateDirFailed {
        path: data_dir.clone(),
        source: e,
    })?;

    let id = uuid::Uuid::new_v4().to_string();
    std::fs::write(device_id_path, &id).map_err(|e| DeviceIdError::WriteFailed {
        path: device_id_path.clone(),
        source: e,
    })?;

    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_device_id_on_first_run() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().join("saku");
        let device_id_path = data_dir.join("device_id");

        let id = create_device_id(&data_dir.clone(), &device_id_path.clone()).unwrap();

        assert!(!id.is_empty());
        // Verify it's a valid UUID
        uuid::Uuid::parse_str(&id).expect("should be a valid UUID");
        // Verify file was created
        let content = std::fs::read_to_string(&device_id_path).unwrap();
        assert_eq!(content, id);
    }

    #[test]
    fn returns_same_id_on_subsequent_reads() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().join("saku");
        let device_id_path = data_dir.join("device_id");

        let id1 = create_device_id(&data_dir.clone(), &device_id_path.clone()).unwrap();

        // Read again — should get same value
        let content = std::fs::read_to_string(&device_id_path).unwrap();
        assert_eq!(content.trim(), id1);
    }

    #[test]
    fn saku_data_dir_returns_path() {
        let dir = saku_data_dir().unwrap();
        assert!(dir.ends_with("saku"));
    }
}
