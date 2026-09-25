#![forbid(unsafe_code)]

use std::time::Duration;

use chrono::{SecondsFormat, Utc};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use uuid::Uuid;

use crate::{
    protocol::peer::{
        AckMessageRequest, AckMessageResponse, AcquireLeaseRequest, AcquireLeaseResponse,
        EnqueueMessageRequest, EnqueueMessageResponse, PeerMessageDto, PollInboxRequest,
        PollInboxResponse, ReleaseLeaseRequest, ReleaseLeaseResponse, RenewLeaseRequest,
    },
    Storage,
};

use super::ServiceError;

const DEFAULT_POLL_LIMIT: u32 = 50;
const MAX_POLL_LIMIT: u32 = 100;

#[derive(Clone)]
pub struct PeerService {
    storage: Storage,
}

impl PeerService {
    pub fn new(storage: Storage) -> Self {
        Self { storage }
    }

    pub fn enqueue_message(
        &self,
        req: EnqueueMessageRequest,
    ) -> Result<EnqueueMessageResponse, ServiceError> {
        require_non_empty("from_name", &req.from_name)?;
        require_non_empty("to_name", &req.to_name)?;
        require_non_empty("target_session_id", &req.target_session_id)?;
        require_non_empty("body", &req.body)?;

        let message_id = format!("msg-{}", Uuid::new_v4());
        let thread_id = match req.correlation_id.as_deref() {
            Some(correlation_id) => {
                require_non_empty("correlation_id", correlation_id)?;
                correlation_id.to_owned()
            }
            None => format!("thread-{}", Uuid::new_v4()),
        };
        let intent = req.intent.unwrap_or_else(|| "question".to_owned());
        require_non_empty("intent", &intent)?;
        let priority = req.priority.unwrap_or(0);
        let timestamp = now_timestamp();

        let mut connection = self.storage.lock_connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT OR IGNORE INTO peer_threads (thread_id, created_at) VALUES (?1, ?2)",
            params![thread_id, timestamp],
        )?;
        transaction.execute(
            "INSERT INTO peer_messages (
                message_id, thread_id, from_agent, to_agent, body, intent, priority, state,
                created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', ?8)",
            params![
                message_id,
                thread_id,
                req.from_name,
                req.target_session_id,
                req.body,
                intent,
                priority,
                timestamp
            ],
        )?;
        transaction.commit()?;

        Ok(EnqueueMessageResponse {
            ok: true,
            message_id,
            state: "pending".to_owned(),
            timestamp,
        })
    }

    pub fn poll_inbox(&self, req: PollInboxRequest) -> Result<PollInboxResponse, ServiceError> {
        require_non_empty("session_id", &req.session_id)?;
        let limit = req.limit.unwrap_or(DEFAULT_POLL_LIMIT).min(MAX_POLL_LIMIT) as i64;
        let connection = self.storage.lock_connection()?;
        let anchor = match req.after_id.as_deref() {
            Some(after_id) => Some(
                connection
                    .query_row(
                        "SELECT created_at FROM peer_messages
                         WHERE message_id = ?1 AND to_agent = ?2",
                        params![after_id, req.session_id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?
                    .ok_or_else(|| ServiceError::NotFound(format!("message {after_id}")))?,
            ),
            None => None,
        };

        let mut statement = connection.prepare(
            "SELECT message_id, thread_id, from_agent, to_agent, body, intent, priority, state,
                    delivery_receipt, processing_receipt, created_at
             FROM peer_messages
             WHERE to_agent = ?1
               AND (?2 IS NULL OR created_at > ?2 OR (created_at = ?2 AND message_id > ?3))
             ORDER BY created_at ASC, message_id ASC
             LIMIT ?4",
        )?;
        let messages = statement
            .query_map(
                params![req.session_id, anchor, req.after_id, limit],
                peer_message_from_row,
            )?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(PollInboxResponse { messages })
    }

    pub fn ack_message(&self, req: AckMessageRequest) -> Result<AckMessageResponse, ServiceError> {
        require_non_empty("message_id", &req.message_id)?;
        require_non_empty("session_id", &req.session_id)?;
        let state = match req.receipt_type.as_str() {
            "delivery" => "delivered",
            "processing" => "processed",
            other => {
                return Err(ServiceError::InvalidRequest(format!(
                    "unsupported receipt_type {other}"
                )))
            }
        };

        let mut connection = self.storage.lock_connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let target_session = transaction
            .query_row(
                "SELECT to_agent FROM peer_messages WHERE message_id = ?1",
                params![req.message_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| ServiceError::NotFound(format!("message {}", req.message_id)))?;
        if target_session != req.session_id {
            return Err(ServiceError::InvalidRequest(
                "session_id does not match the message recipient".to_owned(),
            ));
        }

        let timestamp = now_timestamp();
        match req.receipt_type.as_str() {
            "delivery" => {
                transaction.execute(
                    "UPDATE peer_messages
                     SET delivery_receipt = ?1,
                         state = CASE WHEN state = 'processed' THEN state ELSE 'delivered' END
                     WHERE message_id = ?2",
                    params![timestamp, req.message_id],
                )?;
            }
            "processing" => {
                transaction.execute(
                    "UPDATE peer_messages
                     SET processing_receipt = ?1, state = 'processed'
                     WHERE message_id = ?2",
                    params![timestamp, req.message_id],
                )?;
            }
            _ => unreachable!("receipt type validated above"),
        }
        let persisted_state = transaction.query_row(
            "SELECT state FROM peer_messages WHERE message_id = ?1",
            params![req.message_id],
            |row| row.get::<_, String>(0),
        )?;
        transaction.commit()?;

        debug_assert!(persisted_state == state || persisted_state == "processed");
        Ok(AckMessageResponse {
            ok: true,
            message_id: req.message_id,
            state: persisted_state,
        })
    }

    pub fn acquire_lease(
        &self,
        req: AcquireLeaseRequest,
    ) -> Result<AcquireLeaseResponse, ServiceError> {
        validate_lease_request(&req.resource_id, &req.holder_id)?;
        let acquired = self.storage.acquire_lease(
            &req.resource_id,
            &req.holder_id,
            Duration::from_millis(req.ttl_ms),
        )?;
        self.lease_response(acquired, req.resource_id, req.holder_id)
    }

    pub fn renew_lease(
        &self,
        req: RenewLeaseRequest,
    ) -> Result<AcquireLeaseResponse, ServiceError> {
        validate_lease_request(&req.resource_id, &req.holder_id)?;
        let acquired = self.storage.renew_lease(
            &req.resource_id,
            &req.holder_id,
            Duration::from_millis(req.ttl_ms),
        )?;
        self.lease_response(acquired, req.resource_id, req.holder_id)
    }

    pub fn release_lease(
        &self,
        req: ReleaseLeaseRequest,
    ) -> Result<ReleaseLeaseResponse, ServiceError> {
        validate_lease_request(&req.resource_id, &req.holder_id)?;
        let released = self
            .storage
            .release_lease(&req.resource_id, &req.holder_id)?;
        Ok(ReleaseLeaseResponse {
            released,
            resource_id: req.resource_id,
            holder_id: req.holder_id,
        })
    }

    fn lease_response(
        &self,
        acquired: bool,
        resource_id: String,
        holder_id: String,
    ) -> Result<AcquireLeaseResponse, ServiceError> {
        let expires_at = if acquired {
            self.storage
                .lock_connection()?
                .query_row(
                    "SELECT expires_at FROM leases WHERE resource_id = ?1 AND holder_id = ?2",
                    params![resource_id, holder_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
        } else {
            None
        };
        Ok(AcquireLeaseResponse {
            acquired,
            resource_id,
            holder_id,
            expires_at,
        })
    }
}

fn peer_message_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PeerMessageDto> {
    Ok(PeerMessageDto {
        message_id: row.get(0)?,
        thread_id: row.get(1)?,
        from_agent: row.get(2)?,
        to_agent: row.get(3)?,
        body: row.get(4)?,
        intent: row.get(5)?,
        priority: row.get(6)?,
        state: row.get(7)?,
        delivery_receipt: row.get(8)?,
        processing_receipt: row.get(9)?,
        created_at: row.get(10)?,
    })
}

fn validate_lease_request(resource_id: &str, holder_id: &str) -> Result<(), ServiceError> {
    require_non_empty("resource_id", resource_id)?;
    require_non_empty("holder_id", holder_id)
}

fn require_non_empty(field: &str, value: &str) -> Result<(), ServiceError> {
    if value.trim().is_empty() {
        return Err(ServiceError::InvalidRequest(format!(
            "{field} must not be empty"
        )));
    }
    Ok(())
}

fn now_timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Nanos, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service() -> PeerService {
        PeerService::new(Storage::open_in_memory().expect("open in-memory storage"))
    }

    fn enqueue_request(body: &str, correlation_id: Option<&str>) -> EnqueueMessageRequest {
        EnqueueMessageRequest {
            from_name: "sender-agent".to_owned(),
            from_session_id: Some("sender-session".to_owned()),
            to_name: "recipient-agent".to_owned(),
            target_session_id: "recipient-session".to_owned(),
            body: body.to_owned(),
            intent: Some("question".to_owned()),
            priority: Some(3),
            urgency: Some("normal".to_owned()),
            correlation_id: correlation_id.map(str::to_owned),
        }
    }

    #[test]
    fn enqueue_message_persists_pending_message_and_thread() {
        let service = service();
        let response = service
            .enqueue_message(enqueue_request("hello", Some("thread-known")))
            .expect("enqueue message");

        assert!(response.ok);
        assert!(response.message_id.starts_with("msg-"));
        assert!(Uuid::parse_str(response.message_id.trim_start_matches("msg-")).is_ok());
        assert_eq!(response.state, "pending");

        let connection = service.storage.lock_connection().expect("lock storage");
        let stored = connection
            .query_row(
                "SELECT thread_id, from_agent, to_agent, body, intent, priority, state, created_at
                 FROM peer_messages WHERE message_id = ?1",
                params![response.message_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, i32>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                    ))
                },
            )
            .expect("read stored message");
        assert_eq!(stored.0, "thread-known");
        assert_eq!(stored.1, "sender-agent");
        assert_eq!(stored.2, "recipient-session");
        assert_eq!(stored.3, "hello");
        assert_eq!(stored.4, "question");
        assert_eq!(stored.5, 3);
        assert_eq!(stored.6, "pending");
        assert_eq!(stored.7, response.timestamp);
    }

    #[test]
    fn poll_inbox_returns_messages_in_creation_order_and_honors_cursor() {
        let service = service();
        let first = service
            .enqueue_message(enqueue_request("first", None))
            .expect("enqueue first");
        let second = service
            .enqueue_message(enqueue_request("second", None))
            .expect("enqueue second");

        let all = service
            .poll_inbox(PollInboxRequest {
                session_id: "recipient-session".to_owned(),
                limit: Some(500),
                after_id: None,
            })
            .expect("poll all messages");
        assert_eq!(all.messages.len(), 2);
        assert_eq!(all.messages[0].message_id, first.message_id);
        assert_eq!(all.messages[1].message_id, second.message_id);
        assert_eq!(all.messages[0].to_agent, "recipient-session");

        let after_first = service
            .poll_inbox(PollInboxRequest {
                session_id: "recipient-session".to_owned(),
                limit: None,
                after_id: Some(first.message_id),
            })
            .expect("poll after first");
        assert_eq!(after_first.messages.len(), 1);
        assert_eq!(after_first.messages[0].message_id, second.message_id);
    }

    #[test]
    fn delivery_ack_sets_receipt_and_delivered_state() {
        let service = service();
        let message = service
            .enqueue_message(enqueue_request("deliver", None))
            .expect("enqueue message");
        let response = service
            .ack_message(AckMessageRequest {
                message_id: message.message_id.clone(),
                session_id: "recipient-session".to_owned(),
                receipt_type: "delivery".to_owned(),
            })
            .expect("ack delivery");

        assert_eq!(response.state, "delivered");
        let connection = service.storage.lock_connection().expect("lock storage");
        let receipt = connection
            .query_row(
                "SELECT delivery_receipt, state FROM peer_messages WHERE message_id = ?1",
                params![message.message_id],
                |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, String>(1)?)),
            )
            .expect("read delivery receipt");
        assert!(receipt.0.is_some());
        assert_eq!(receipt.1, "delivered");
    }

    #[test]
    fn processing_ack_sets_receipt_and_processed_state() {
        let service = service();
        let message = service
            .enqueue_message(enqueue_request("process", None))
            .expect("enqueue message");
        let response = service
            .ack_message(AckMessageRequest {
                message_id: message.message_id.clone(),
                session_id: "recipient-session".to_owned(),
                receipt_type: "processing".to_owned(),
            })
            .expect("ack processing");

        assert_eq!(response.state, "processed");
        let connection = service.storage.lock_connection().expect("lock storage");
        let receipt = connection
            .query_row(
                "SELECT processing_receipt, state FROM peer_messages WHERE message_id = ?1",
                params![message.message_id],
                |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, String>(1)?)),
            )
            .expect("read processing receipt");
        assert!(receipt.0.is_some());
        assert_eq!(receipt.1, "processed");
    }

    #[test]
    fn lease_lifecycle_acquires_rejects_competitor_renews_and_releases() {
        let service = service();
        let resource_id = "inbox:recipient-session";

        let acquired = service
            .acquire_lease(AcquireLeaseRequest {
                resource_id: resource_id.to_owned(),
                holder_id: "worker-a".to_owned(),
                ttl_ms: 30_000,
            })
            .expect("acquire lease");
        assert!(acquired.acquired);
        assert!(acquired.expires_at.is_some());

        let rejected = service
            .acquire_lease(AcquireLeaseRequest {
                resource_id: resource_id.to_owned(),
                holder_id: "worker-b".to_owned(),
                ttl_ms: 30_000,
            })
            .expect("reject competing lease");
        assert!(!rejected.acquired);
        assert!(rejected.expires_at.is_none());

        let renewed = service
            .renew_lease(RenewLeaseRequest {
                resource_id: resource_id.to_owned(),
                holder_id: "worker-a".to_owned(),
                ttl_ms: 60_000,
            })
            .expect("renew lease");
        assert!(renewed.acquired);
        assert!(renewed.expires_at.is_some());

        let released = service
            .release_lease(ReleaseLeaseRequest {
                resource_id: resource_id.to_owned(),
                holder_id: "worker-a".to_owned(),
            })
            .expect("release lease");
        assert!(released.released);

        let reacquired = service
            .acquire_lease(AcquireLeaseRequest {
                resource_id: resource_id.to_owned(),
                holder_id: "worker-b".to_owned(),
                ttl_ms: 30_000,
            })
            .expect("reacquire released lease");
        assert!(reacquired.acquired);
    }
}
