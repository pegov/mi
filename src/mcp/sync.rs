use std::future::Future;
use std::pin::Pin;
use std::sync::mpsc::{self as std_mpsc, Sender as StdSender};
use std::sync::{Arc, Mutex};
use std::thread;

use rmcp::handler::client::ClientHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ClientCapabilities, ClientConfig, Implementation,
    ProgressNotificationParam, ProtocolVersion, Tool,
};
use rmcp::service::{NotificationContext, RoleClient, RunningService};
use rmcp::transport::{ConfigureCommandExt, StreamableHttpClientTransport, TokioChildProcess};
use rmcp::{ClientLifecycleMode, ClientServiceExt};
use tokio::process::Command;
use tokio::runtime::Builder;
use tokio::sync::mpsc::{self as tokio_mpsc, Receiver as TokioReceiver, Sender as TokioSender};

type JobFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

struct RuntimeBridge {
    tx: StdSender<JobFuture>,
    join: Option<thread::JoinHandle<()>>,
}

impl RuntimeBridge {
    fn new() -> Self {
        let (tx, rx) = std_mpsc::channel::<JobFuture>();

        let join = thread::spawn(move || {
            let rt = Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build tokio runtime");

            while let Ok(fut) = rx.recv() {
                rt.block_on(fut);
            }
        });

        Self {
            tx,
            join: Some(join),
        }
    }

    fn block_on<F, T>(&self, fut: F) -> T
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let (reply_tx, reply_rx) = std_mpsc::channel::<T>();
        let job: JobFuture = Box::pin(async move {
            let out = fut.await;
            let _ = reply_tx.send(out);
        });
        self.tx.send(job).expect("worker alive");
        reply_rx.recv().expect("worker replied")
    }
}

impl Drop for RuntimeBridge {
    fn drop(&mut self) {
        let (dead_tx, _) = std_mpsc::channel();
        let _ = std::mem::replace(&mut self.tx, dead_tx);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

#[derive(Clone)]
pub struct ProgressHandler {
    tx: TokioSender<ProgressNotificationParam>,
}

impl ProgressHandler {
    pub fn new(tx: TokioSender<ProgressNotificationParam>) -> Self {
        Self { tx }
    }
}

impl ClientHandler for ProgressHandler {
    fn get_info(&self) -> ClientConfig {
        ClientConfig::new(
            ClientCapabilities::default(),
            Implementation::from_build_env(),
        )
    }

    async fn on_progress(
        &self,
        params: ProgressNotificationParam,
        _ctx: NotificationContext<RoleClient>,
    ) {
        let _ = self.tx.try_send(params);
    }
}

pub struct SyncMcpClient {
    bridge: RuntimeBridge,
    service: RunningService<RoleClient, ProgressHandler>,
    progress_rx: Mutex<TokioReceiver<ProgressNotificationParam>>,
}

pub enum Endpoint<'a> {
    Stdio(&'a str),
    Http(&'a str),
}

impl SyncMcpClient {
    pub fn connect_any(ep: Endpoint) -> anyhow::Result<Self> {
        match ep {
            Endpoint::Stdio(p) => Self::connect(p),
            Endpoint::Http(u) => Self::connect_http(u),
        }
    }

    pub fn connect(program: &str) -> anyhow::Result<Self> {
        let bridge = RuntimeBridge::new();
        let program = program.to_owned();

        let (tx, progress_rx) = tokio_mpsc::channel(64);
        let handler = ProgressHandler::new(tx);

        let service = bridge.block_on(async move {
            let transport = TokioChildProcess::new(Command::new(&program).configure(|_| {}))?;

            let client = handler
                .serve_with_lifecycle(
                    transport,
                    ClientLifecycleMode::Discover {
                        preferred_versions: vec![ProtocolVersion::V_2026_07_28],
                    },
                )
                .await?;

            Ok::<_, anyhow::Error>(client)
        })?;

        Ok(Self {
            bridge,
            service,
            progress_rx: Mutex::new(progress_rx),
        })
    }

    pub fn connect_http(url: &str) -> anyhow::Result<Self> {
        let bridge = RuntimeBridge::new();
        let url = url.to_owned();

        let (tx, progress_rx) = tokio_mpsc::channel(64);
        let handler = ProgressHandler::new(tx);

        let service = bridge.block_on(async move {
            let transport = StreamableHttpClientTransport::from_uri(Arc::from(url));

            let client = handler
                .serve_with_lifecycle(
                    transport,
                    ClientLifecycleMode::Discover {
                        preferred_versions: vec![ProtocolVersion::V_2026_07_28],
                    },
                )
                .await?;

            Ok::<_, anyhow::Error>(client)
        })?;

        Ok(Self {
            bridge,
            service,
            progress_rx: Mutex::new(progress_rx),
        })
    }

    pub fn list_tools(&self) -> anyhow::Result<Vec<Tool>> {
        let svc = self.service.clone();
        self.bridge
            .block_on(async move { svc.list_all_tools().await })
            .map_err(Into::into)
    }

    pub fn call_tool(&self, name: &str, args: serde_json::Value) -> anyhow::Result<CallToolResult> {
        let svc = self.service.clone();
        let params = CallToolRequestParams::new(name.to_string())
            .with_arguments(args.as_object().cloned().unwrap_or_default());

        self.bridge
            .block_on(async move { svc.call_tool(params).await })
            .map_err(Into::into)
    }

    pub fn try_recv_progress(&self) -> Option<ProgressNotificationParam> {
        self.progress_rx.lock().ok()?.try_recv().ok()
    }

    pub fn recv_progress(&self) -> Option<ProgressNotificationParam> {
        self.progress_rx.lock().ok()?.blocking_recv()
    }
}
