//! 入口：仅负责启动 tokio 运行时并进入应用事件循环。

#[tokio::main]
async fn main() -> std::io::Result<()> {
    linklens::app::run().await
}
