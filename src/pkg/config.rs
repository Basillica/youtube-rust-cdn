use super::error::CdnError;
use std::fs;

pub fn load_config(file_path: &str) -> Result<String, CdnError> {
    fs::read_to_string(file_path).map_err(CdnError::from)
}
