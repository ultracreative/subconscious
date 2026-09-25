#![forbid(unsafe_code)]

use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

use crate::{
    protocol::council::{
        EvaluateCouncilRequest, EvaluateCouncilResponse, ReconcileCouncilRequest,
        ReconcileCouncilResponse, StageCouncilRequest, StageCouncilResponse,
    },
    Storage,
};

use super::ServiceError;

const TERMINAL_STATUSES: [&str; 3] = ["completed", "failed", "cancelled"];
const EVALUATION_STATUSES: [&str; 4] = ["running", "completed", "failed", "cancelled"];

#[derive(Clone)]
pub struct CouncilService {
    storage: Storage,
}

impl CouncilService {
    pub fn new(storage: Storage) -> Self {
        Self { storage }
    }

    pub fn stage(&self, req: StageCouncilRequest) -> Result<StageCouncilResponse, ServiceError> {
        require_non_empty("council_id", &req.council_id)?;
        require_non_empty("name", &req.name)?;
        require_non_empty("question", &req.question)?;
        if req.members.is_empty() {
            return Err(ServiceError::InvalidRequest(
                "members must not be empty".to_owned(),
            ));
        }
        for member in &req.members {
            require_non_empty("member", member)?;
        }

        let members_json = serde_json::to_string(&req.members)
            .map_err(|error| invalid_json("council members", error))?;
        let started_at = now_timestamp();
        let mut connection = self.storage.lock_connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO council_runs (
                council_id, name, question, intent, mode, members_json, status, started_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'staged', ?7)",
            params![
                req.council_id,
                req.name,
                req.question,
                req.intent,
                req.mode,
                members_json,
                started_at
            ],
        )?;
        for member in &req.members {
            transaction.execute(
                "INSERT INTO council_member_states (
                    council_id, member_name, status, updated_at
                 ) VALUES (?1, ?2, 'staged', ?3)",
                params![req.council_id, member, started_at],
            )?;
        }
        transaction.commit()?;

        Ok(StageCouncilResponse {
            ok: true,
            council_id: req.council_id,
            status: "staged".to_owned(),
            started_at,
        })
    }

    pub fn evaluate(
        &self,
        req: EvaluateCouncilRequest,
    ) -> Result<EvaluateCouncilResponse, ServiceError> {
        require_non_empty("council_id", &req.council_id)?;
        require_non_empty("member_name", &req.member_name)?;
        if !EVALUATION_STATUSES.contains(&req.status.as_str()) {
            return Err(ServiceError::InvalidRequest(format!(
                "invalid council member status '{}'",
                req.status
            )));
        }

        let members_json = {
            let connection = self.storage.lock_connection()?;
            connection
                .query_row(
                    "SELECT members_json FROM council_runs WHERE council_id = ?1",
                    params![req.council_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .ok_or_else(|| ServiceError::NotFound(format!("council {}", req.council_id)))?
        };
        let declared_members: Vec<String> = serde_json::from_str(&members_json)
            .map_err(|error| invalid_json("stored council members", error))?;

        self.storage.record_member_state(
            &req.council_id,
            &req.member_name,
            &req.status,
            req.response_block.as_deref(),
            req.error.as_deref(),
        )?;
        let member_refs = declared_members
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let all_members_terminal = self
            .storage
            .all_members_terminal(&req.council_id, &member_refs)?;

        Ok(EvaluateCouncilResponse {
            ok: true,
            council_id: req.council_id,
            member_name: req.member_name,
            status: req.status,
            all_members_terminal,
        })
    }

    pub fn reconcile(
        &self,
        req: ReconcileCouncilRequest,
    ) -> Result<ReconcileCouncilResponse, ServiceError> {
        require_non_empty("council_id", &req.council_id)?;
        {
            let connection = self.storage.lock_connection()?;
            require_council(&connection, &req.council_id)?;
        }

        let member_refs = req
            .declared_members
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        if !self
            .storage
            .all_members_terminal(&req.council_id, &member_refs)?
        {
            let connection = self.storage.lock_connection()?;
            if let Some((member, status)) =
                first_non_terminal_member(&connection, &req.council_id, &req.declared_members)?
            {
                return Err(non_terminal_error(&member, &status));
            }
        }

        let completed_at = now_timestamp();
        let outcome_json = serde_json::json!({
            "synthesis": req.synthesis,
            "agreement_level": req.agreement_level,
        })
        .to_string();
        let mut connection = self.storage.lock_connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_council(&transaction, &req.council_id)?;
        if let Some((member, status)) =
            first_non_terminal_member(&transaction, &req.council_id, &req.declared_members)?
        {
            return Err(non_terminal_error(&member, &status));
        }
        transaction.execute(
            "UPDATE council_runs
             SET status = 'completed', completed_at = ?1, outcome_json = ?2
             WHERE council_id = ?3",
            params![completed_at, outcome_json, req.council_id],
        )?;
        transaction.commit()?;

        Ok(ReconcileCouncilResponse {
            ok: true,
            council_id: req.council_id,
            status: "completed".to_owned(),
            completed_at,
        })
    }
}

fn require_council(connection: &Connection, council_id: &str) -> Result<(), ServiceError> {
    connection
        .query_row(
            "SELECT 1 FROM council_runs WHERE council_id = ?1",
            params![council_id],
            |_| Ok(()),
        )
        .optional()?
        .ok_or_else(|| ServiceError::NotFound(format!("council {council_id}")))
}

fn first_non_terminal_member(
    connection: &Connection,
    council_id: &str,
    declared_members: &[String],
) -> Result<Option<(String, String)>, ServiceError> {
    let mut statement = connection.prepare(
        "SELECT status FROM council_member_states
         WHERE council_id = ?1 AND member_name = ?2",
    )?;
    for member in declared_members {
        let status = statement
            .query_row(params![council_id, member], |row| row.get::<_, String>(0))
            .optional()?;
        match status {
            Some(status) if TERMINAL_STATUSES.contains(&status.as_str()) => {}
            Some(status) => return Ok(Some((member.clone(), status))),
            None => return Ok(Some((member.clone(), "missing".to_owned()))),
        }
    }
    Ok(None)
}

fn non_terminal_error(member: &str, status: &str) -> ServiceError {
    ServiceError::InvalidRequest(format!(
        "Cannot reconcile: member '{member}' has non-terminal status '{status}'"
    ))
}

fn require_non_empty(field: &str, value: &str) -> Result<(), ServiceError> {
    if value.trim().is_empty() {
        return Err(ServiceError::InvalidRequest(format!(
            "{field} must not be empty"
        )));
    }
    Ok(())
}

fn invalid_json(context: &str, error: serde_json::Error) -> ServiceError {
    ServiceError::InvalidRequest(format!("{context} JSON is invalid: {error}"))
}

fn now_timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Nanos, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service() -> CouncilService {
        CouncilService::new(Storage::open_in_memory().expect("open in-memory storage"))
    }

    fn stage_request(council_id: &str) -> StageCouncilRequest {
        StageCouncilRequest {
            council_id: council_id.to_owned(),
            name: "Release council".to_owned(),
            question: "Should the release proceed?".to_owned(),
            intent: "decision".to_owned(),
            mode: "deliberation".to_owned(),
            members: vec!["alpha".to_owned(), "beta".to_owned(), "gamma".to_owned()],
            prompt: "Review readiness and identify blockers.".to_owned(),
            context_files: None,
            guidance: Some("Attribute conclusions to council members.".to_owned()),
            deadline_ms: Some(60_000),
        }
    }

    fn evaluate(
        service: &CouncilService,
        council_id: &str,
        member_name: &str,
        status: &str,
        response: Option<&str>,
        error: Option<&str>,
    ) -> EvaluateCouncilResponse {
        service
            .evaluate(EvaluateCouncilRequest {
                council_id: council_id.to_owned(),
                member_name: member_name.to_owned(),
                status: status.to_owned(),
                response_block: response.map(str::to_owned),
                error: error.map(str::to_owned),
                token_cost_nanodollars: None,
            })
            .expect("evaluate council member")
    }

    fn reconcile_request(council_id: &str) -> ReconcileCouncilRequest {
        ReconcileCouncilRequest {
            council_id: council_id.to_owned(),
            declared_members: vec!["alpha".to_owned(), "beta".to_owned(), "gamma".to_owned()],
            synthesis: "alpha approved; beta reported a provider failure; gamma cancelled."
                .to_owned(),
            agreement_level: Some("mixed".to_owned()),
        }
    }

    #[test]
    fn staging_multi_member_council_records_run_and_initial_member_states() {
        let service = service();
        let request = stage_request("council-stage");
        let expected_members = request.members.clone();
        let response = service.stage(request).expect("stage council");

        assert!(response.ok);
        assert_eq!(response.council_id, "council-stage");
        assert_eq!(response.status, "staged");
        assert!(chrono::DateTime::parse_from_rfc3339(&response.started_at).is_ok());

        let connection = service.storage.lock_connection().expect("lock storage");
        let run = connection
            .query_row(
                "SELECT name, question, intent, mode, members_json, status, started_at
                 FROM council_runs WHERE council_id = ?1",
                params![response.council_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                    ))
                },
            )
            .expect("read council run");
        assert_eq!(run.0, "Release council");
        assert_eq!(run.1, "Should the release proceed?");
        assert_eq!(run.2, "decision");
        assert_eq!(run.3, "deliberation");
        assert_eq!(
            serde_json::from_str::<Vec<String>>(&run.4).expect("decode members"),
            expected_members
        );
        assert_eq!(run.5, "staged");
        assert_eq!(run.6, response.started_at);

        let mut statement = connection
            .prepare(
                "SELECT member_name, status, response_text, error_text, updated_at
                 FROM council_member_states WHERE council_id = ?1 ORDER BY member_name",
            )
            .expect("prepare member query");
        let states = statement
            .query_map(params![response.council_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .expect("query member states")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect member states");
        assert_eq!(states.len(), 3);
        assert_eq!(
            states
                .iter()
                .map(|state| (state.0.as_str(), state.1.as_str()))
                .collect::<Vec<_>>(),
            vec![("alpha", "staged"), ("beta", "staged"), ("gamma", "staged")]
        );
        assert!(states
            .iter()
            .all(|state| state.2.is_none() && state.3.is_none() && state.4 == response.started_at));
    }

    #[test]
    fn evaluating_members_updates_state_and_reports_when_all_are_terminal() {
        let service = service();
        service
            .stage(stage_request("council-evaluate"))
            .expect("stage council");

        let running = evaluate(&service, "council-evaluate", "alpha", "running", None, None);
        assert!(!running.all_members_terminal);
        let completed = evaluate(
            &service,
            "council-evaluate",
            "alpha",
            "completed",
            Some("Approve with canary safeguards."),
            None,
        );
        assert!(!completed.all_members_terminal);
        let failed = evaluate(
            &service,
            "council-evaluate",
            "beta",
            "failed",
            None,
            Some("Provider unavailable"),
        );
        assert!(!failed.all_members_terminal);
        let cancelled = evaluate(
            &service,
            "council-evaluate",
            "gamma",
            "cancelled",
            None,
            None,
        );
        assert!(cancelled.all_members_terminal);

        let connection = service.storage.lock_connection().expect("lock storage");
        let alpha = connection
            .query_row(
                "SELECT status, response_text, error_text FROM council_member_states
                 WHERE council_id = ?1 AND member_name = 'alpha'",
                params!["council-evaluate"],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .expect("read alpha state");
        assert_eq!(alpha.0, "completed");
        assert_eq!(alpha.1.as_deref(), Some("Approve with canary safeguards."));
        assert_eq!(alpha.2, None);

        let beta = connection
            .query_row(
                "SELECT status, response_text, error_text FROM council_member_states
                 WHERE council_id = ?1 AND member_name = 'beta'",
                params!["council-evaluate"],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .expect("read beta state");
        assert_eq!(beta.0, "failed");
        assert_eq!(beta.1, None);
        assert_eq!(beta.2.as_deref(), Some("Provider unavailable"));
    }

    #[test]
    fn reconcile_rejects_running_and_staged_members() {
        let service = service();
        service
            .stage(stage_request("council-blocked"))
            .expect("stage council");
        evaluate(
            &service,
            "council-blocked",
            "alpha",
            "completed",
            Some("Approve"),
            None,
        );
        evaluate(&service, "council-blocked", "beta", "running", None, None);

        let running_error = service
            .reconcile(reconcile_request("council-blocked"))
            .expect_err("running member must block reconcile");
        assert!(matches!(
            running_error,
            ServiceError::InvalidRequest(ref message)
                if message == "Cannot reconcile: member 'beta' has non-terminal status 'running'"
        ));

        evaluate(
            &service,
            "council-blocked",
            "beta",
            "failed",
            None,
            Some("Provider unavailable"),
        );
        let staged_error = service
            .reconcile(reconcile_request("council-blocked"))
            .expect_err("staged member must block reconcile");
        assert!(matches!(
            staged_error,
            ServiceError::InvalidRequest(ref message)
                if message == "Cannot reconcile: member 'gamma' has non-terminal status 'staged'"
        ));

        let connection = service.storage.lock_connection().expect("lock storage");
        let run = connection
            .query_row(
                "SELECT status, completed_at, outcome_json FROM council_runs WHERE council_id = ?1",
                params!["council-blocked"],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .expect("read blocked council");
        assert_eq!(run, ("staged".to_owned(), None, None));
    }

    #[test]
    fn reconcile_completes_after_all_members_are_terminal_and_persists_synthesis() {
        let service = service();
        service
            .stage(stage_request("council-complete"))
            .expect("stage council");
        evaluate(
            &service,
            "council-complete",
            "alpha",
            "completed",
            Some("Approve with safeguards"),
            None,
        );
        evaluate(
            &service,
            "council-complete",
            "beta",
            "failed",
            None,
            Some("Provider unavailable"),
        );
        evaluate(
            &service,
            "council-complete",
            "gamma",
            "cancelled",
            None,
            None,
        );

        let request = reconcile_request("council-complete");
        let expected_synthesis = request.synthesis.clone();
        let response = service.reconcile(request).expect("reconcile council");
        assert!(response.ok);
        assert_eq!(response.council_id, "council-complete");
        assert_eq!(response.status, "completed");
        assert!(chrono::DateTime::parse_from_rfc3339(&response.completed_at).is_ok());

        let connection = service.storage.lock_connection().expect("lock storage");
        let run = connection
            .query_row(
                "SELECT status, completed_at, outcome_json FROM council_runs WHERE council_id = ?1",
                params![response.council_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .expect("read completed council");
        assert_eq!(run.0, "completed");
        assert_eq!(run.1, response.completed_at);
        let outcome: serde_json::Value =
            serde_json::from_str(&run.2).expect("decode council outcome");
        assert_eq!(outcome["synthesis"], expected_synthesis);
        assert_eq!(outcome["agreement_level"], "mixed");
    }
}
