use tokio_cron_scheduler::{JobScheduler, Job};
use crate::{AppState, models::RecordingSchedule};
use std::sync::Arc;
use std::collections::HashMap;
use uuid::Uuid;
use chrono_tz::Asia::Tokyo;

pub struct SchedulerManager {
    scheduler: JobScheduler,
    job_map: Arc<tokio::sync::Mutex<HashMap<i32, Uuid>>>, // schedule_id -> job_uuid
}

impl SchedulerManager {
    pub async fn new() -> Result<Self, String> {
        let scheduler = JobScheduler::new().await
            .map_err(|e| format!("Failed to create scheduler: {}", e))?;

        scheduler.start().await
            .map_err(|e| format!("Failed to start scheduler: {}", e))?;

        println!("[Scheduler] Scheduler started successfully");

        Ok(Self {
            scheduler,
            job_map: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        })
    }

    pub async fn add_schedule(
        &self,
        schedule: RecordingSchedule,
        state: Arc<AppState>
    ) -> Result<Uuid, String> {
        let schedule_id = schedule.id;
        let camera_id = schedule.camera_id;
        let duration = schedule.duration_minutes;
        let fps = schedule.fps;
        let cron_expr = schedule.cron_expression.clone();
        let name = schedule.name.clone();

        println!("[Scheduler] Adding schedule '{}' (ID: {}) with cron: {}", name, schedule_id, cron_expr);

        let job = Job::new_async_tz(cron_expr.as_str(), Tokyo, move |_uuid, _lock| {
            let state_clone = state.clone();
            let camera_id = camera_id;
            let duration = duration;
            let fps = fps;
            let name = name.clone();

            Box::pin(async move {
                println!("[Scheduler] !!!!! EXECUTING SCHEDULE '{}' FOR CAMERA {} !!!!!", name, camera_id);

                // Start scheduled recording
                if let Err(e) = start_scheduled_recording(
                    state_clone.clone(),
                    camera_id,
                    duration,
                    fps
                ).await {
                    eprintln!("[Scheduler] Failed to start recording for '{}': {}", name, e);
                    return;
                }

                println!("[Scheduler] Recording started for '{}', will stop after {} minutes", name, duration);

                // Wait for duration and then stop
                tokio::time::sleep(tokio::time::Duration::from_secs((duration * 60) as u64)).await;

                if let Err(e) = stop_scheduled_recording(state_clone.clone(), camera_id).await {
                    eprintln!("[Scheduler] Failed to stop recording for '{}': {}", name, e);
                } else {
                    println!("[Scheduler] Recording completed for '{}'", name);
                }
            })
        }).map_err(|e| format!("Failed to create job: {}", e))?;

        let job_id = job.guid();

        self.scheduler.add(job).await
            .map_err(|e| format!("Failed to add job to scheduler: {}", e))?;

        // Store the mapping
        let mut map = self.job_map.lock().await;
        map.insert(schedule_id, job_id);

        println!("[Scheduler] Schedule added successfully: {} -> {}", schedule_id, job_id);

        Ok(job_id)
    }

    pub async fn remove_schedule(&self, schedule_id: i32) -> Result<(), String> {
        let mut map = self.job_map.lock().await;

        if let Some(job_id) = map.remove(&schedule_id) {
            println!("[Scheduler] Removing schedule {} (job {})", schedule_id, job_id);
            self.scheduler.remove(&job_id).await
                .map_err(|e| format!("Failed to remove job from scheduler: {}", e))?;
            println!("[Scheduler] Schedule removed successfully");
            Ok(())
        } else {
            Err(format!("Schedule {} not found in job map", schedule_id))
        }
    }

    pub async fn get_job_id(&self, schedule_id: i32) -> Option<Uuid> {
        let map = self.job_map.lock().await;
        map.get(&schedule_id).copied()
    }
}

// Helper function to start scheduled recording
async fn start_scheduled_recording(
    state: Arc<AppState>,
    camera_id: i32,
    _duration_minutes: i32,
    fps: Option<i32>
) -> Result<(), String> {
    // Directly call the stream function with state components
    crate::stream::start_recording_with_options_direct(
        &state,
        camera_id,
        fps
    ).await
}

// Helper function to stop scheduled recording
async fn stop_scheduled_recording(
    state: Arc<AppState>,
    camera_id: i32
) -> Result<(), String> {
    crate::stream::stop_recording_direct(&state, camera_id).await
}
