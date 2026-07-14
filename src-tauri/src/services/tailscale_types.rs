use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TailscaleStatus {
    pub state: String,
    pub device_name: Option<String>,
    pub tailnet_ip: Option<String>,
    pub magic_dns_name: Option<String>,
    pub login_url: Option<String>,
    pub access_urls: Vec<String>,
    pub serving: bool,
    #[serde(default)]
    pub public: bool,
    #[serde(default)]
    pub exposure_mode: Option<String>,
    #[serde(default)]
    pub public_port: Option<u16>,
    pub message: Option<String>,
}

impl TailscaleStatus {
    pub fn disabled() -> Self {
        Self {
            state: "disabled".to_string(),
            device_name: None,
            tailnet_ip: None,
            magic_dns_name: None,
            login_url: None,
            access_urls: Vec::new(),
            serving: false,
            public: false,
            exposure_mode: Some("private".to_string()),
            public_port: None,
            message: None,
        }
    }

    pub fn stopped(message: impl Into<String>) -> Self {
        Self {
            state: "stopped".to_string(),
            device_name: None,
            tailnet_ip: None,
            magic_dns_name: None,
            login_url: None,
            access_urls: Vec::new(),
            serving: false,
            public: false,
            exposure_mode: Some("private".to_string()),
            public_port: None,
            message: Some(message.into()),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            state: "error".to_string(),
            device_name: None,
            tailnet_ip: None,
            magic_dns_name: None,
            login_url: None,
            access_urls: Vec::new(),
            serving: false,
            public: false,
            exposure_mode: Some("private".to_string()),
            public_port: None,
            message: Some(message.into()),
        }
    }

    pub fn needs_login(message: impl Into<String>) -> Self {
        Self {
            state: "needsLogin".to_string(),
            device_name: None,
            tailnet_ip: None,
            magic_dns_name: None,
            login_url: None,
            access_urls: Vec::new(),
            serving: false,
            public: false,
            exposure_mode: Some("private".to_string()),
            public_port: None,
            message: Some(message.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TailscaleLogin {
    pub login_url: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TailscaleStartRequest {
    pub state_dir: String,
    pub hostname: String,
    pub auth_key: Option<String>,
    pub backend_addr: String,
    pub serve_port: u16,
    #[serde(default)]
    pub public: bool,
}
