use std::{
    fmt::{Debug, Formatter, Result as FmtResult},
    path::PathBuf,
};

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    pub prefix: String,
    pub database_path: Option<PathBuf>,
    pub twitch: Twitch,
    #[cfg(feature = "obs")]
    pub obs: Obs,
}

#[derive(Clone, Deserialize)]
pub struct Twitch {
    pub client_id: String,
    pub client_secret: String,
    pub name: String,
    pub channels: Vec<String>,
}

impl Debug for Twitch {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("Twitch")
            .field("client_id", &"hidden")
            .field("client_secret", &"hidden")
            .field("name", &self.name)
            .field("channels", &self.channels)
            .finish()
    }
}

#[derive(Clone, Deserialize)]
#[cfg(feature = "obs")]
pub struct Obs {
    pub websocket_port: u16,
    pub websocket_password: String,
}

#[cfg(feature = "obs")]
impl Debug for Obs {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("Obs")
            .field("websocket_port", &self.websocket_port)
            .field("websocket_password", &"hidden")
            .finish()
    }
}
