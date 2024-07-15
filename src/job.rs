use super::messages::{AcedrgArgs,JobId};
use actix::prelude::*;
// use futures_util::FutureExt;
use job_runner::JobRunner;
use std::{collections::BTreeMap, time::Duration};
pub mod job_runner;

pub const ACEDRG_OUTPUT_FILENAME: &'static str = "acedrg_output";

#[derive(Clone, Debug)]
pub struct JobOutput {
    pub stdout: String,
    pub stderr: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobStatus {
    Pending,
    Finished,
    Failed(JobFailureReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobFailureReason {
    TimedOut,
    IOError(std::io::ErrorKind),
    AcedrgError,
}

#[derive(Debug, Clone)]
pub struct JobData {
    pub status: JobStatus,
    /// Gets filled when the job completes.
    /// If the job fails, it will only be filled
    /// if the error came from acedrg itself
    pub job_output: Option<JobOutput>,
}
impl Message for JobData {
    type Result = ();
}

pub struct JobManager {
    jobs: BTreeMap<JobId, Addr<JobRunner>>,
}

impl Actor for JobManager {
    type Context = Context<Self>;
}

pub struct NewJob(pub AcedrgArgs);
impl Message for NewJob {
    type Result = std::io::Result<(JobId, Addr<JobRunner>)>;
}

pub struct QueryJob(pub JobId);
impl Message for QueryJob {
    type Result = Option<Addr<JobRunner>>;
}

impl Handler<QueryJob> for JobManager {
    type Result = <QueryJob as actix::Message>::Result;

    fn handle(&mut self, msg: QueryJob, _ctx: &mut Self::Context) -> Self::Result {
        //log::debug!("Jobs={:?}", self.jobs.keys().collect::<Vec<_>>());
        self.jobs.get(&msg.0).cloned()
    }
}

struct RemoveJob(pub JobId);
impl Message for RemoveJob {
    type Result = ();
}

impl Handler<RemoveJob> for JobManager {
    type Result = <RemoveJob as actix::Message>::Result;

    fn handle(&mut self, msg: RemoveJob, _ctx: &mut Self::Context) -> Self::Result {
        self.jobs.remove(&msg.0);
        log::info!("Removed job with ID={}", msg.0);
    }
}

impl Handler<NewJob> for JobManager {
    type Result = ResponseActFuture<Self, <NewJob as actix::Message>::Result>;

    fn handle(&mut self, msg: NewJob, _ctx: &mut Self::Context) -> Self::Result {
        let id = loop {
            let new_id = uuid::Uuid::new_v4();
            let id = new_id.to_string();
            if ! self.jobs.contains_key(&id) {
                break id;
            }
        };
        Box::pin(async move {
            let args = msg.0;
            // todo: sanitize input in create_job()!!!

            JobRunner::create_job(id.clone(), vec![], &args).await
                .map(|addr| (id, addr))
            // Err::<(String, Addr<JobRunner>), std::io::Error>(std::io::Error::new(std::io::ErrorKind::Other, "j"))
        }.into_actor(self).map(|job_res, _actor , ctx| {
            job_res.map(|(jid, job)| {
                self.jobs.insert(jid.clone(), job.clone());

                // Cleanup task
                // Make sure to keep this longer than the job timeout
                // We don't have to care if the job is still running or not.
                // In the worst-case scenario, it should have timed-out a long time ago.
                ctx.notify_later(RemoveJob(jid.clone()), Duration::from_secs(15 * 60));
        
                log::info!("Added job with ID={}", &jid);
                (jid, job)
            })
        }))
    }
}

impl JobManager {
    pub fn new() -> Self {
        log::info!("Initializing JobManager.");
        Self {
            jobs: BTreeMap::new(),
        }
    }
}
