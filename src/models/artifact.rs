use clap::ValueEnum;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum StorageProvider {
    Artifactory,
    Cloudsmith,
}

impl Default for StorageProvider {
    fn default() -> Self {
        StorageProvider::Artifactory
    }
}
