use serde::{Serialize, Deserialize};

include!(concat!(env!("OUT_DIR"), "/version_constants.rs"));

#[derive(Serialize, Deserialize)]
pub(crate) struct VersionData {
    version: String,
    commit_hash: String,
    is_dirty: bool,
}

impl From<VersionData> for clap::builder::Str {
    fn from(version_data: VersionData) -> Self {
        let json_version_data_res = serde_json::to_string(&version_data);
        match json_version_data_res {
            Ok(data) => clap::builder::Str::from(data),
            Err(err) => panic!("Could not deserialize VersionData, error code: {}", err)
        }
    }
}

pub(crate) fn current_version() -> VersionData {

    let version_data = VersionData {
        version: env!("CARGO_PKG_VERSION").to_string(),
        commit_hash: GIT_COMMIT_HASH.to_string(),
        is_dirty: GIT_IS_DIRTY,
    };

    version_data
}