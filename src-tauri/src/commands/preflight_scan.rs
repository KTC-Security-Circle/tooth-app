use anyhow::Context;
use serde::Serialize;
use std::path::Path;

use crate::{
    errors::AppError,
    state::{scan_configuration::CalibrationProfileManifest, settings::Settings},
    utils::config::calibration_profile_path,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreflightCheck {
    pub name: String,
    pub passed: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreflightResult {
    pub ready: bool,
    pub checks: Vec<PreflightCheck>,
}

pub fn validate_scan_configuration(settings: &Settings) -> Result<PreflightResult, AppError> {
    let mut checks = Vec::new();
    check_dir(&mut checks, "dataRoot", &settings.data_root, "data root");
    check_file(
        &mut checks,
        "fixedReferencePly",
        &settings.fixed_reference_ply,
        "fixed reference PLY",
    );
    check_file(
        &mut checks,
        "stereoCalibrationFile",
        &settings.stereo_calibration_file,
        "stereo calibration file",
    );
    check_file(
        &mut checks,
        "calibrationProfile",
        &settings.calibration_profile_path,
        "calibration profile",
    );
    if settings.data_root.is_empty()
        || settings.fixed_reference_ply.is_empty()
        || settings.stereo_calibration_file.is_empty()
        || settings.calibration_profile_path.is_empty()
    {
        return Err(AppError::Validation(
            "data root, calibration profile, stereo calibration file, and fixed reference PLY are required before scanning"
                .to_string(),
        ));
    }
    let profile_path = Path::new(&settings.calibration_profile_path);
    let profile_result = CalibrationProfileManifest::load(profile_path)
        .and_then(|profile| profile.validate_files(profile_path))
        .with_context(|| {
            format!(
                "validating calibration profile '{}'",
                profile_path.display()
            )
        });
    match profile_result {
        Ok(()) => {}
        Err(error) => {
            log::error!("{error:#}");
            return Err(AppError::Validation(
                "calibration profile could not be read or is invalid; check the profile and its artifact files"
                    .to_string(),
            ));
        }
    };
    checks.push(PreflightCheck {
        name: "profileArtifacts".to_string(),
        passed: true,
        message: "calibration profile artifacts are present".to_string(),
    });
    let ready = checks.iter().all(|check| check.passed);
    Ok(PreflightResult { ready, checks })
}

fn check_dir(checks: &mut Vec<PreflightCheck>, name: &str, value: &str, label: &str) {
    let path = Path::new(value);
    let passed = !value.is_empty() && path.is_dir() && is_writable(path);
    checks.push(PreflightCheck {
        name: name.to_string(),
        passed,
        message: if passed {
            format!("{label} is available")
        } else {
            format!("{label} is missing, is not a directory, or is not writable: {value}")
        },
    });
}

fn is_writable(path: &Path) -> bool {
    let probe = path.join(format!(".tooth-preflight-{}", std::process::id()));
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(probe);
            true
        }
        Err(_) => false,
    }
}

fn check_file(checks: &mut Vec<PreflightCheck>, name: &str, value: &str, label: &str) {
    let passed =
        !value.is_empty() && Path::new(value).is_file() && std::fs::File::open(value).is_ok();
    checks.push(PreflightCheck {
        name: name.to_string(),
        passed,
        message: if passed {
            format!("{label} is available")
        } else {
            format!("{label} is missing or unreadable: {value}")
        },
    });
}

#[tauri::command]
pub async fn preflight_scan(app: tauri::AppHandle) -> Result<PreflightResult, AppError> {
    let settings = Settings::load(&app)?;
    // Resolve the default profile without starting a sidecar or touching hardware.
    let settings = if settings.calibration_profile_path.is_empty() {
        let mut settings = settings;
        settings.calibration_profile_path = calibration_profile_path(&app)?
            .to_string_lossy()
            .into_owned();
        settings
    } else {
        settings
    };
    validate_scan_configuration(&settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::settings::Settings;

    #[test]
    fn reports_missing_static_inputs_without_hardware_work() {
        let error = validate_scan_configuration(&Settings::default())
            .expect_err("empty settings must fail");
        assert!(matches!(error, AppError::Validation(message) if message.contains("data root")));
    }

    #[test]
    fn hides_profile_parser_details_from_validation_error() {
        let root = std::env::temp_dir().join(format!("tooth-preflight-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&root);
        let profile = root.join("profile.json");
        let _ = std::fs::write(&profile, "not json");
        let settings = Settings {
            data_root: root.to_string_lossy().into_owned(),
            fixed_reference_ply: root.join("reference.ply").to_string_lossy().into_owned(),
            stereo_calibration_file: root.join("stereo.json").to_string_lossy().into_owned(),
            calibration_profile_path: profile.to_string_lossy().into_owned(),
            ..Settings::default()
        };
        let error = validate_scan_configuration(&settings).expect_err("invalid profile must fail");
        assert!(matches!(error, AppError::Validation(message)
            if message == "calibration profile could not be read or is invalid; check the profile and its artifact files"));
        let _ = std::fs::remove_dir_all(root);
    }
}
