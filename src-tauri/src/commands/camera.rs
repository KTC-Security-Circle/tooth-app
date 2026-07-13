use crate::errors::AppError;
use serde::Serialize;

#[derive(Serialize)]
pub struct CameraInfo {
    pub name: String,
    pub path: String,
}

#[tauri::command]
pub fn list_cameras() -> Result<Vec<CameraInfo>, AppError> {
    let mut cameras = Vec::new();
    for node in v4l::context::enum_devices() {
        let path = node.path().to_string_lossy().to_string();
        match v4l::Device::with_path(&path) {
            Ok(dev) => match dev.query_caps() {
                Ok(caps) => {
                    if caps
                        .capabilities
                        .contains(v4l::capability::Flags::VIDEO_CAPTURE)
                    {
                        cameras.push(CameraInfo {
                            name: caps.card.clone(),
                            path,
                        });
                    }
                }
                Err(e) => eprintln!("Warning: cannot query caps for {}: {}", path, e),
            },
            Err(e) => eprintln!("Warning: cannot open {}: {}", path, e),
        }
    }
    Ok(cameras)
}
