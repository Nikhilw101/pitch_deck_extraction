use crate::models::deck_model::ProcessingResponse;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JobStatus {
    Processing { progress: String },
    Completed(ProcessingResponse),
    Failed { error: String },
}

pub struct JobService {
    jobs: DashMap<String, JobStatus>,
}

impl Default for JobService {
    fn default() -> Self {
        Self::new()
    }
}

impl JobService {
    pub fn new() -> Self {
        Self {
            jobs: DashMap::new(),
        }
    }

    pub fn create_job(&self) -> String {
        let job_id = Uuid::new_v4().to_string();
        self.jobs.insert(
            job_id.clone(),
            JobStatus::Processing {
                progress: "Initializing job".to_string(),
            },
        );
        job_id
    }

    pub fn update_progress(&self, job_id: &str, progress: &str) {
        if let Some(mut job) = self.jobs.get_mut(job_id) {
            *job = JobStatus::Processing {
                progress: progress.to_string(),
            };
        }
    }

    pub fn complete_job(&self, job_id: &str, response: ProcessingResponse) {
        if let Some(mut job) = self.jobs.get_mut(job_id) {
            *job = JobStatus::Completed(response);
        }
    }

    pub fn fail_job(&self, job_id: &str, error: String) {
        if let Some(mut job) = self.jobs.get_mut(job_id) {
            *job = JobStatus::Failed { error };
        }
    }

    pub fn get_job_status(&self, job_id: &str) -> Option<JobStatus> {
        self.jobs.get(job_id).map(|entry| entry.value().clone())
    }
}
