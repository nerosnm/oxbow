use std::{fmt::Debug, time::Duration};

use obws::{requests::SceneItemRender, Client};
use thiserror::Error;
use tokio::time::sleep;
use tracing::{debug, error, info, instrument};

pub struct ObsHandler {
    pub(in crate::bot) port: u16,
    pub(in crate::bot) password: String,
}

impl ObsHandler {
    #[instrument(skip(self))]
    pub async fn obs_loop(&mut self) {
        debug!("starting");

        match self.obs().await {
            Ok(()) => {}
            Err(err) => error!(%err),
        }
    }

    #[instrument(skip(self))]
    async fn obs(&mut self) -> Result<(), ObsError> {
        let mut client = Client::connect("localhost", self.port).await?;

        let version = client.general().get_version().await?;
        debug!(obws_version = ?version.version, ?version.obs_websocket_version, ?version.obs_studio_version, "connected");

        client.login(Some(self.password.clone())).await?;
        info!("logged in successfully");

        self.show_notification(&mut client, "Harris Carrot", Duration::from_secs(5))
            .await?;

        Ok(())
    }

    #[instrument(skip(self, client))]
    async fn show_notification<S>(
        &mut self,
        client: &mut Client,
        source: S,
        duration: Duration,
    ) -> Result<(), ObsError>
    where
        S: AsRef<str> + Debug,
    {
        info!("showing notification");

        self.show_source(client, source.as_ref()).await?;
        sleep(duration).await;
        self.hide_source(client, source.as_ref()).await?;

        Ok(())
    }

    #[instrument(skip(self, client))]
    async fn show_source<S>(&self, client: &mut Client, source: S) -> Result<(), ObsError>
    where
        S: AsRef<str> + Debug,
    {
        debug!("showing source");

        let scene_item_render = SceneItemRender {
            scene_name: None,
            source: source.as_ref(),
            item: None,
            render: true,
        };

        client
            .scene_items()
            .set_scene_item_render(scene_item_render)
            .await?;

        Ok(())
    }

    #[instrument(skip(self, client))]
    async fn hide_source<S>(&self, client: &mut Client, source: S) -> Result<(), ObsError>
    where
        S: AsRef<str> + Debug,
    {
        debug!("hiding source");

        let scene_item_render = SceneItemRender {
            scene_name: None,
            source: source.as_ref(),
            item: None,
            render: false,
        };

        client
            .scene_items()
            .set_scene_item_render(scene_item_render)
            .await?;

        Ok(())
    }
}

#[derive(Debug, Error)]
enum ObsError {
    #[error("obws error: {0}")]
    ObwsError(#[from] obws::Error),
}
