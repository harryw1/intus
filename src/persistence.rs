use crate::ollama::ChatMessage;
use std::fs;
use std::path::PathBuf;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

pub enum PersistenceEvent {
    Save(PathBuf, Vec<ChatMessage>),
    Flush(oneshot::Sender<()>),
}

#[derive(Debug)]
pub struct SessionManager {
    tx: mpsc::UnboundedSender<PersistenceEvent>,
}

impl SessionManager {
    pub fn new() -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<PersistenceEvent>();

        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                match event {
                    PersistenceEvent::Save(path, messages) => {
                        let _ = tokio::task::spawn_blocking(move || {
                            if let Ok(json) = serde_json::to_string(&messages) {
                                if let Some(parent) = path.parent() {
                                    let _ = fs::create_dir_all(parent);
                                }
                                if path.exists() {
                                    let backup_path = path.with_extension("json.bak");
                                    let _ = fs::copy(&path, &backup_path);
                                }
                                let temp_path = path.with_extension("json.tmp");
                                if fs::write(&temp_path, &json).is_ok() {
                                    let _ = fs::rename(&temp_path, &path);
                                }
                            }
                        })
                        .await;
                    }
                    PersistenceEvent::Flush(reply_tx) => {
                        let _ = reply_tx.send(());
                    }
                }
            }
        });

        Self { tx }
    }

    pub fn save_session(&self, path: PathBuf, messages: Vec<ChatMessage>) {
        let _ = self.tx.send(PersistenceEvent::Save(path, messages));
    }

    pub async fn wait_for_save(&self) {
        let (tx, rx) = oneshot::channel();
        if self.tx.send(PersistenceEvent::Flush(tx)).is_ok() {
            let _ = rx.await;
        }
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}
