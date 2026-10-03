//! 简短命令入口，与主命令共用应用逻辑。

#[tokio::main]
async fn main() -> std::io::Result<()> {
    network_tui::app::dispatch().await
}
