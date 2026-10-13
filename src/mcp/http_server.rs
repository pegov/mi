use rmcp::RoleServer;
use rmcp::model::{Implementation, ProtocolVersion, ServerCapabilities, ServerConfig};
use rmcp::model::{NumberOrString, ProgressNotificationParam, ProgressToken};
use rmcp::service::RequestContext;
use rmcp::tool_handler;
use rmcp::{
    ServerHandler,
    handler::server::wrapper::Parameters,
    schemars, tool, tool_router,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::never::NeverSessionManager,
    },
};
use std::sync::Arc;
use tokio::runtime::Builder;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct HttpEchoParams {
    text: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct StreamParams {
    count: u32,
}

#[derive(Clone)]
struct HttpEchoServer;

#[tool_router]
impl HttpEchoServer {
    #[tool(description = "Echo the input text")]
    fn echo(&self, Parameters(HttpEchoParams { text }): Parameters<HttpEchoParams>) -> String {
        text
    }

    #[tool(description = "Stream a countdown with progress notifications")]
    async fn stream_countdown(
        &self,
        Parameters(StreamParams { count }): Parameters<StreamParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<String, rmcp::ErrorData> {
        let token = ctx
            .meta
            .get_progress_token()
            .unwrap_or_else(|| ProgressToken(NumberOrString::String("stream".into())));

        for i in 1..=count {
            let param = ProgressNotificationParam::new(token.clone(), i as f64)
                .with_total(count as f64)
                .with_message(format!("step {i} of {count}"));

            ctx.peer.notify_progress(param).await.map_err(|e| {
                rmcp::ErrorData::internal_error(format!("progress notify failed: {e}"), None)
            })?;

            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        }

        Ok(format!("countdown from {count} complete"))
    }
}

#[tool_handler]
impl ServerHandler for HttpEchoServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_protocol_version(ProtocolVersion::V_2026_07_28)
            .with_server_info(Implementation::from_build_env())
    }
}

pub fn mcp_http_server(addr: &str, mount_path: &str) -> anyhow::Result<()> {
    let addr = addr.to_owned();
    let mount_path = mount_path.to_owned();

    let handle = std::thread::spawn(move || -> anyhow::Result<()> {
        let rt = Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("build tokio runtime");

        rt.block_on(async move {
            let config = StreamableHttpServerConfig::default()
                .with_legacy_session_mode(false)
                .with_stateless_protocol_metadata_required(true);

            let service = StreamableHttpService::new(
                || Ok(HttpEchoServer),
                Arc::new(NeverSessionManager::default()),
                config,
            );

            let router = axum::Router::new().nest_service(&mount_path, service);
            let listener = tokio::net::TcpListener::bind(&addr).await?;
            println!("Echo MCP server listening on http://{addr}{mount_path}");

            axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    tokio::signal::ctrl_c().await.ok();
                })
                .await?;

            Ok::<_, anyhow::Error>(())
        })
    });

    handle.join().expect("server thread panicked")
}
