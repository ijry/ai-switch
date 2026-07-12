use crate::models::account::NewOfficialAccount;
use crate::models::provider::NewProvider;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExampleImportPayload {
    #[serde(default)]
    pub providers: Vec<NewProvider>,
    #[serde(default)]
    pub accounts: Vec<NewOfficialAccount>,
}

pub fn parse_example_json(input: &str) -> Result<ExampleImportPayload, serde_json::Error> {
    serde_json::from_str(input)
}
