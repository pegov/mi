use rmcp::{
    ServiceExt, handler::server::wrapper::Parameters, schemars, tool, tool_router,
    transport::io::stdio,
};

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct EchoParams {
    text: String,
}

#[derive(Clone)]
struct EchoServer;

#[tool_router(server_handler)]
impl EchoServer {
    #[tool(description = "Echo the input text")]
    fn echo(&self, Parameters(EchoParams { text }): Parameters<EchoParams>) -> String {
        text
    }
}

pub fn echo_mcp_server() -> anyhow::Result<()> {
    let handle = std::thread::spawn(|| {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        rt.block_on(async {
            let service = EchoServer.serve(stdio()).await?;
            service.waiting().await?;
            Ok::<_, anyhow::Error>(())
        })
    });

    handle.join().unwrap()?;
    Ok(())
}
