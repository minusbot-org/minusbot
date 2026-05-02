use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, Utc};
use minus_db::job::JobRecord;
use minus_db::Database;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::{mpsc, Notify, RwLock};
use uuid::Uuid;

/// Describes when a job should run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScheduleKind {
    /// Run once at a specific time
    OnceAt(DateTime<Utc>),
    /// Run after a delay from creation
    Delay(std::time::Duration),
    /// Run at a cron schedule
    Cron(String),
    /// Run at fixed intervals
    Interval(std::time::Duration),
}

/// What the job does when triggered.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JobAction {
    AgentPrompt { prompt: String, agent_id: Option<String> },
    MessageSend { content: String, generate: bool },
    UseChat { chat_id: String, content: String },
    AskAgent { chat_id: String, content: String, agent_id: String },
}

/// A triggered job event sent to the runtime.
#[derive(Debug, Clone)]
pub struct JobTrigger {
    pub job_id: String,
    pub job_name: String,
    pub actions: Vec<JobAction>,
    pub target_chat_id: Option<String>,
}

pub struct Scheduler {
    db: Database,
    trigger_tx: mpsc::Sender<JobTrigger>,
    jobs: Arc<RwLock<HashMap<String, JobRecord>>>,
    notify: Arc<Notify>,
}

impl Scheduler {
    pub fn new(db: Database, trigger_tx: mpsc::Sender<JobTrigger>) -> Self {
        Self {
            db,
            trigger_tx,
            jobs: Arc::new(RwLock::new(HashMap::new())),
            notify: Arc::new(Notify::new()),
        }
    }

    /// Load all jobs from the database into memory.
    pub async fn load_jobs(&self) -> Result<()> {
        let jobs = self.db.list_jobs().await?;
        let mut map = self.jobs.write().await;
        for job in jobs {
            map.insert(job.id.clone(), job);
        }
        tracing::info!(count = map.len(), "Loaded jobs into memory");
        Ok(())
    }

    pub async fn create_job(
        &self,
        name: &str,
        schedule_str: &str,
        content: &str,
        target_chat_id: Option<&str>,
        generate: bool,
    ) -> Result<String> {
        self.create_job_v2(
            name,
            schedule_str,
            vec![JobAction::MessageSend {
                content: content.to_string(),
                generate,
            }],
            target_chat_id,
        )
        .await
    }

    /// Create a new job with specific actions.
    pub async fn create_job_v2(
        &self,
        name: &str,
        schedule_str: &str,
        actions: Vec<JobAction>,
        target_chat_id: Option<&str>,
    ) -> Result<String> {
        let (kind, expr) = parse_schedule(schedule_str)?;
        let next_run = compute_next_run(&kind)?;

        let job_id = Uuid::new_v4().to_string();
        let action_kind = "multi_action";
        let action_json = serde_json::to_string(&actions)?;
        let next_run_str = next_run.map(|t| t.to_rfc3339());

        let (kind_str, expr_str) = match &kind {
            ScheduleKind::OnceAt(_) => ("once_at", schedule_str.to_string()),
            ScheduleKind::Delay(_) => ("delay", expr.clone()),
            ScheduleKind::Cron(_) => ("cron", expr.clone()),
            ScheduleKind::Interval(_) => ("interval", expr.clone()),
        };

        self.db
            .create_job(
                &job_id,
                name,
                None,
                kind_str,
                &expr_str,
                action_kind,
                &action_json,
                target_chat_id,
                next_run_str.as_deref(),
            )
            .await?;

        // Update memory
        let now = Utc::now().to_rfc3339();
        let job = JobRecord {
            id: job_id.clone(),
            name: name.to_string(),
            description: None,
            schedule_kind: kind_str.to_string(),
            schedule_expr: expr_str,
            action_kind: action_kind.to_string(),
            action_json,
            target_chat_id: target_chat_id.map(|s| s.to_string()),
            enabled: true,
            created_at: now.clone(),
            updated_at: now,
            next_run_at: next_run_str,
        };

        {
            let mut map = self.jobs.write().await;
            map.insert(job_id.clone(), job);
        }
        self.notify.notify_one();

        tracing::info!(job_id = %job_id, name = %name, schedule = %schedule_str, action = %action_kind, "Job created and cached");
        Ok(job_id)
    }

    /// Cancel a job by ID.
    pub async fn cancel_job(&self, job_id: &str) -> Result<bool> {
        if self.db.cancel_job(job_id).await? {
            let mut map = self.jobs.write().await;
            if let Some(job) = map.get_mut(job_id) {
                job.enabled = false;
                job.updated_at = Utc::now().to_rfc3339();
            }
            self.notify.notify_one();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// List all jobs.
    pub async fn list_jobs(&self) -> Result<Vec<JobRecord>> {
        let map = self.jobs.read().await;
        let mut list: Vec<_> = map.values().cloned().collect();
        list.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(list)
    }

    /// Update an existing job.
    pub async fn update_job(
        &self,
        job_id: &str,
        name: &str,
        schedule_str: &str,
        actions: Vec<JobAction>,
    ) -> Result<()> {
        let (kind, expr) = parse_schedule(schedule_str)?;
        let next_run = compute_next_run(&kind)?;

        let action_kind = "multi_action";
        let action_json = serde_json::to_string(&actions)?;
        let next_run_str = next_run.map(|t| t.to_rfc3339());

        let (kind_str, expr_str) = match &kind {
            ScheduleKind::OnceAt(_) => ("once_at", schedule_str.to_string()),
            ScheduleKind::Delay(_) => ("delay", expr.clone()),
            ScheduleKind::Cron(_) => ("cron", expr.clone()),
            ScheduleKind::Interval(_) => ("interval", expr.clone()),
        };

        self.db
            .update_job(
                job_id,
                name,
                kind_str,
                &expr_str,
                action_kind,
                &action_json,
                next_run_str.as_deref(),
            )
            .await?;

        // Update memory
        {
            let mut map = self.jobs.write().await;
            if let Some(job) = map.get_mut(job_id) {
                job.name = name.to_string();
                job.schedule_kind = kind_str.to_string();
                job.schedule_expr = expr_str;
                job.action_kind = action_kind.to_string();
                job.action_json = action_json;
                job.next_run_at = next_run_str;
                job.updated_at = Utc::now().to_rfc3339();
            }
        }
        self.notify.notify_one();

        tracing::info!(job_id = %job_id, name = %name, schedule = %schedule_str, action = %action_kind, "Job updated and cached");
        Ok(())
    }

    /// Start the scheduler tick loop. This runs in the background.
    pub fn start(self: Arc<Self>, mut shutdown: tokio::sync::broadcast::Receiver<()>) {
        let s = self.clone();
        tokio::spawn(async move {
            tracing::info!("Scheduler thread started");

            // Initial load
            if let Err(e) = s.load_jobs().await {
                tracing::error!(error = %e, "Initial job load failed");
            }

            loop {
                // Calculate sleep duration based on memory state
                let next_run = {
                    let map = s.jobs.read().await;
                    map.values()
                        .filter(|j| j.enabled && j.next_run_at.is_some())
                        .filter_map(|j| {
                            DateTime::parse_from_rfc3339(j.next_run_at.as_ref().unwrap())
                                .ok()
                                .map(|t| t.with_timezone(&Utc))
                        })
                        .min()
                };

                let now = Utc::now();
                let sleep_dur = match next_run {
                    Some(t) => {
                        if t > now {
                            (t - now)
                                .to_std()
                                .unwrap_or(std::time::Duration::from_millis(10))
                        } else {
                            std::time::Duration::ZERO
                        }
                    }
                    None => std::time::Duration::from_secs(3600), // Sleep 1h if no jobs
                };

                if sleep_dur > std::time::Duration::ZERO {
                    tracing::debug!(next_run = ?next_run, "Scheduler sleeping for {:?}", sleep_dur);
                }

                tokio::select! {
                    _ = tokio::time::sleep(sleep_dur) => {
                        if let Err(e) = s.tick().await {
                            tracing::error!(error = %e, "Scheduler tick error");
                        }
                    }
                    _ = s.notify.notified() => {
                        tracing::debug!("Scheduler woken up by notification");
                        // Just loop to re-calculate next run
                    }
                    _ = shutdown.recv() => {
                        tracing::info!("Scheduler shutting down");
                        break;
                    }
                }
            }
        });
    }

    /// Check for jobs that need to be triggered.
    async fn tick(&self) -> Result<()> {
        let now = Utc::now();
        let mut triggered = Vec::new();

        // 1. Identify jobs to trigger (from memory)
        {
            let map = self.jobs.read().await;
            for job in map.values() {
                if !job.enabled {
                    continue;
                }
                if let Some(next_run_str) = &job.next_run_at {
                    if let Ok(next_run) = DateTime::parse_from_rfc3339(next_run_str) {
                        let next_run = next_run.with_timezone(&Utc);
                        if now >= next_run {
                            triggered.push(job.clone());
                        }
                    }
                }
            }
        }

        // 2. Process triggered jobs
        for job in triggered {
            tracing::info!(job_id = %job.id, name = %job.name, "Triggering job");

            let actions: Vec<JobAction> = serde_json::from_str(&job.action_json)
                .or_else(|_| {
                    // Fallback for single action
                    serde_json::from_str::<JobAction>(&job.action_json).map(|a| vec![a])
                })
                .unwrap_or_else(|_| vec![JobAction::MessageSend {
                    content: "Job triggered (invalid action format)".into(),
                    generate: false,
                }]);

            let trigger = JobTrigger {
                job_id: job.id.clone(),
                job_name: job.name.clone(),
                actions,
                target_chat_id: job.target_chat_id.clone(),
            };

            if let Err(e) = self.trigger_tx.send(trigger).await {
                tracing::error!(error = %e, "Failed to send job trigger");
            }

            // 3. Update next run time
            let mut next_run_at = None;
            let mut enabled = job.enabled;

            match job.schedule_kind.as_str() {
                "delay" | "once_at" => {
                    enabled = false;
                }
                "cron" => {
                    if let Ok(schedule) = cron::Schedule::from_str(&job.schedule_expr) {
                        next_run_at = schedule.upcoming(Utc).next();
                    }
                }
                "interval" => {
                    if let Ok(dur) = parse_duration_str(&job.schedule_expr) {
                        if let Ok(chrono_dur) = Duration::from_std(dur) {
                            next_run_at = Some(now + chrono_dur);
                        }
                    }
                }
                _ => {}
            }

            let next_run_str = next_run_at.map(|t| t.to_rfc3339());

            // Persist to DB
            if !enabled {
                let _ = self.db.cancel_job(&job.id).await;
            } else {
                let _ = self
                    .db
                    .update_job_next_run(&job.id, next_run_str.as_deref())
                    .await;
            }

            // Update memory
            {
                let mut map = self.jobs.write().await;
                if let Some(j) = map.get_mut(&job.id) {
                    j.enabled = enabled;
                    j.next_run_at = next_run_str;
                    j.updated_at = Utc::now().to_rfc3339();
                }
            }
        }

        Ok(())
    }
}

/// Parse a schedule string like "delay:10s", "cron:0 9 * * *", "interval:1h"
pub fn parse_schedule(s: &str) -> Result<(ScheduleKind, String)> {
    if let Some(rest) = s.strip_prefix("delay:") {
        let dur = parse_duration_str(rest)?;
        Ok((ScheduleKind::Delay(dur), rest.to_string()))
    } else if let Some(rest) = s.strip_prefix("cron:") {
        // Validate cron
        let _ = cron::Schedule::from_str(rest)
            .with_context(|| format!("Invalid cron expression: {}", rest))?;
        Ok((ScheduleKind::Cron(rest.to_string()), rest.to_string()))
    } else if let Some(rest) = s.strip_prefix("interval:") {
        let dur = parse_duration_str(rest)?;
        Ok((ScheduleKind::Interval(dur), rest.to_string()))
    } else {
        bail!(
            "Unknown schedule format: '{}'. Use delay:, cron:, or interval:",
            s
        )
    }
}

fn compute_next_run(kind: &ScheduleKind) -> Result<Option<DateTime<Utc>>> {
    let now = Utc::now();
    match kind {
        ScheduleKind::OnceAt(t) => Ok(Some(*t)),
        ScheduleKind::Delay(d) => Ok(Some(now + Duration::from_std(*d)?)),
        ScheduleKind::Cron(expr) => {
            let schedule = cron::Schedule::from_str(expr)?;
            Ok(schedule.upcoming(Utc).next())
        }
        ScheduleKind::Interval(d) => Ok(Some(now + Duration::from_std(*d)?)),
    }
}

/// Parse duration strings like "10s", "5m", "2h", "1d"
pub fn parse_duration_str(s: &str) -> Result<std::time::Duration> {
    let s = s.trim();
    if s.is_empty() {
        bail!("Empty duration string");
    }

    let (num_str, unit) = if s.ends_with('s') {
        (&s[..s.len() - 1], "s")
    } else if s.ends_with('m') {
        (&s[..s.len() - 1], "m")
    } else if s.ends_with('h') {
        (&s[..s.len() - 1], "h")
    } else if s.ends_with('d') {
        (&s[..s.len() - 1], "d")
    } else {
        // Default to seconds
        (s, "s")
    };

    let num: u64 = num_str
        .parse()
        .with_context(|| format!("Invalid duration number: '{}'", num_str))?;

    let secs = match unit {
        "s" => num,
        "m" => num * 60,
        "h" => num * 3600,
        "d" => num * 86400,
        _ => bail!("Unknown duration unit: {}", unit),
    };

    Ok(std::time::Duration::from_secs(secs))
}

#[minus_api::async_trait]
impl minus_api::traits::MinusScheduler for Scheduler {
    async fn list_tasks(&self) -> Result<Vec<minus_api::SchedulerTask>> {
        let jobs = self.list_jobs().await?;
        Ok(jobs.into_iter().map(|j| minus_api::SchedulerTask {
            id: j.id,
            name: j.name,
            schedule: j.schedule_expr,
            action: j.action_kind,
            enabled: j.enabled,
            next_run: j.next_run_at.and_then(|s| DateTime::parse_from_rfc3339(&s).ok()).map(|d| d.with_timezone(&Utc)),
        }).collect())
    }

    async fn delete_task(&self, id: &str) -> Result<bool> {
        self.cancel_job(id).await
    }

    async fn create_task(&self, name: &str, schedule: &str, prompt: &str, target_chat_id: Option<&str>, generate: bool) -> Result<String> {
        self.create_job(name, schedule, prompt, target_chat_id, generate).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration_str("10s").unwrap().as_secs(), 10);
        assert_eq!(parse_duration_str("5m").unwrap().as_secs(), 300);
        assert_eq!(parse_duration_str("2h").unwrap().as_secs(), 7200);
        assert_eq!(parse_duration_str("1d").unwrap().as_secs(), 86400);
    }

    #[test]
    fn test_parse_schedule_delay() {
        let (kind, _) = parse_schedule("delay:30s").unwrap();
        matches!(kind, ScheduleKind::Delay(_));
    }

    #[test]
    fn test_parse_schedule_cron() {
        let (kind, _) = parse_schedule("cron:0 0 9 * * * *").unwrap();
        matches!(kind, ScheduleKind::Cron(_));
    }

    #[test]
    fn test_parse_schedule_invalid() {
        assert!(parse_schedule("unknown:abc").is_err());
    }
}
