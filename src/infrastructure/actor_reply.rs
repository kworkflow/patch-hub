use std::fmt::Display;

use tokio::sync::oneshot;

/// Sends a oneshot actor reply and records when that reply is lost.
pub struct ActorReplyService;

impl ActorReplyService {
    pub fn send_actor_reply<T, E: Display>(
        message_name: &'static str,
        failure_log: &'static str,
        dropped_log: &'static str,
        reply: oneshot::Sender<Result<T, E>>,
        result: Result<T, E>,
    ) {
        if let Err(error) = &result {
            tracing::warn!(
                message = message_name,
                error = %error,
                "{failure_log}"
            );
        }

        if reply.send(result).is_err() {
            tracing::warn!(message = message_name, "{dropped_log}");
        }
    }

    pub fn deliver_value<T>(
        message_name: &'static str,
        dropped_log: &'static str,
        reply: oneshot::Sender<T>,
        value: T,
    ) {
        if reply.send(value).is_err() {
            tracing::warn!(message = message_name, "{dropped_log}");
        }
    }
}
