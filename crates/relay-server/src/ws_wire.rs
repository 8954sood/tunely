use axum::extract::ws::Message;
use protocol::ControlMessage;
use tokio::sync::mpsc;

pub fn send_control(
    sender: &mpsc::UnboundedSender<Message>,
    msg: &ControlMessage,
) -> anyhow::Result<()> {
    let payload = serde_json::to_string(msg)?;
    sender
        .send(Message::Text(payload.into()))
        .map_err(|_| anyhow::anyhow!("agent websocket channel closed"))
}
