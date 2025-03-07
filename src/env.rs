use std::{collections::HashMap, sync::Arc};

use tokio::{
    sync::{mpsc::Sender, Mutex},
    task::JoinHandle,
};

/// Represents a interface to handle environment variables
///
/// It holds no data at all, just call [`Env::spawn`] to crate a brand new
/// actor.
///
/// The expected flow is:
///  - Spawn the actor right away with [`spawn`]
///  - Get, set and unset environment variables
///
/// [`spawn`]: Env::spawn
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EnvCore;

impl EnvCore {
    /// Instantiate a new Env actor that will be executed on a dedicated task.
    /// Return both the actor [`EnvTx`] and a [`JoinHandle`] to permit awaiting
    /// for the actor to be finished
    ///
    /// The handling of messages is done sequentially
    pub fn spawn() -> (Env, JoinHandle<()>) {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Command>(100);
        let handle = tokio::spawn(async move {
            while let Some(command) = rx.recv().await {
                match command {
                    Command::Get(key, tx) => {
                        let _ = tx.send(std::env::var(key));
                    }
                    Command::Set { key, value } => {
                        std::env::set_var(key, value);
                    }
                    Command::Unset(key) => {
                        std::env::remove_var(key);
                    }
                }
            }
        });

        (Env::Default(tx), handle)
    }
}

#[derive(Debug)]
pub enum Command {
    Get(
        String,
        tokio::sync::oneshot::Sender<Result<String, std::env::VarError>>,
    ),
    Set {
        key: String,
        value: String,
    },
    Unset(String),
}

/// The transmitter that sends messages down to a env actor. This is what you should
/// use across the code to get, set or unset environment variables. It's cheap to clone
/// do not be afraid of cloning it.
///
/// This transmitter is obtained by calling [`Env::spawn`] to instantiate a dedicated task
/// to handle messages.
///
/// The intended usage is:
/// - Spawn the env actor with [`Env::spawn`]
/// - Use the methods of this struct to manipulate environment variables
#[derive(Debug, Clone)]
pub enum Env {
    Default(Sender<Command>),
    Mock(Arc<Mutex<HashMap<String, String>>>),
}

impl Env {
    /// Creates a mock version of [`Env`] for testing purposes
    ///
    /// A mock env won't use actual environment variables, but rather a
    /// [`HashMap`]
    #[allow(dead_code)]
    pub fn mock() -> Self {
        Self::Mock(Arc::new(Mutex::new(HashMap::new())))
    }
    /// Retrieves the value of an environment variable
    ///
    /// Returns a [`Result`] with either the variable content or an error.
    /// The error can be:
    ///
    /// 1. The key is invalid
    /// 2. The variable is not set
    pub async fn get(&self, key: impl ToString + Send) -> Result<String, std::env::VarError> {
        match self {
            Self::Mock(map) => map
                .lock()
                .await
                .get(&key.to_string())
                .ok_or(std::env::VarError::NotPresent)
                .cloned(),
            Self::Default(sender) => {
                let (tx, rx) = tokio::sync::oneshot::channel();
                sender
                    .send(Command::Get(key.to_string(), tx))
                    .await
                    .expect("Env actor died");

                rx.await.expect("Env actor died")
            }
        }
    }

    /// Defines the value of the [`key`] environment variable to the string [`value`]
    #[allow(dead_code)]
    pub async fn set(&self, key: impl ToString + Send, value: impl ToString + Send) {
        match self {
            Env::Mock(map) => {
                map.lock().await.insert(key.to_string(), value.to_string());
            }
            Env::Default(sender) => {
                sender
                    .send(Command::Set {
                        key: key.to_string(),
                        value: value.to_string(),
                    })
                    .await
                    .expect("Env actor died");
            }
        }
    }

    /// Undefines an environment variable
    #[allow(dead_code)]
    pub async fn unset(&self, key: impl ToString + Send) {
        match self {
            Env::Mock(mutex) => {
                mutex.lock().await.remove(&key.to_string());
            }
            Env::Default(sender) => {
                sender
                    .send(Command::Unset(key.to_string()))
                    .await
                    .expect("Env actor died");
            }
        };
    }
}
