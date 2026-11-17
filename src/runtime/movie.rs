//! Movie requests suspend the VM until the Android media host completes them.
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_MOVIE: AtomicU64 = AtomicU64::new(1);

#[derive(Serialize)]
pub struct MovieRequest {
    pub id: u64,
    pub resource: String,
    pub volume: f32,
    #[serde(skip)]
    resume_script: bool,
    #[serde(skip)]
    previous_wait: Waiting,
}

impl Runtime {
    pub fn movie_request(&self) -> Option<&MovieRequest> {
        self.movie.as_ref()
    }

    fn queue_movie(&mut self, resource: &str, resume_script: bool) -> Result<()> {
        let resource = resource.replace('\\', "/");
        if resource.is_empty()
            || resource.starts_with('/')
            || resource.contains(':')
            || resource
                .split('/')
                .any(|part| part.is_empty() || matches!(part, "." | ".."))
        {
            return Err("movie resource must be a relative game path".into());
        }
        if self.movie.is_some() {
            return Err("a movie is already active".into());
        }
        self.set_playback_mode("manual")?;
        self.movie = Some(MovieRequest {
            id: NEXT_MOVIE.fetch_add(1, Ordering::Relaxed),
            resource,
            volume: self.movie_volume.clamp(0, 1000) as f32 / 1000.0,
            resume_script,
            previous_wait: self.waiting,
        });
        self.waiting = Waiting::Movie;
        Ok(())
    }

    pub fn request_movie(&mut self, resource: &str) -> Result<()> {
        self.queue_movie(resource, true)
    }

    pub fn request_menu_movie(&mut self, resource: &str) -> Result<()> {
        self.movie_volume = self.global_flag(100).clamp(0, 1000);
        self.queue_movie(resource, false)
    }

    pub fn complete_movie(&mut self, id: u64, error: Option<&str>) -> Result<()> {
        if !self.movie.as_ref().is_some_and(|request| request.id == id) {
            return Ok(());
        }
        let request = self.movie.take().unwrap();
        if let Some(error) = error {
            self.approximations
                .insert(format!("Movie playback failed: {error}"));
        }
        if !request.resume_script {
            self.waiting = request.previous_wait;
        } else if self.paused_at.is_some() {
            // A completion queued during Activity.onPause must not run story commands.
            self.waiting = Waiting::Timer(Instant::now());
        } else {
            self.waiting = Waiting::Ready;
            self.drive()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires the original game through YUKA_TEST_GAME"]
    fn menu_movie_preserves_dialogue_and_ignores_stale_completion() -> Result<()> {
        let game = PathBuf::from(std::env::var("YUKA_TEST_GAME")?);
        let mut runtime = Runtime::new(&game, PLAY_SCRIPT, TextEncoding::Gbk, true, true, 0)?;
        let before = runtime.scene.dialogue.clone();
        let pc = runtime.report().pc;
        assert!(runtime.request_menu_movie("../bad.mpg").is_err());
        runtime.set_paused(true);
        runtime.request_menu_movie("avi\\lave_op.mpg")?;
        assert!(matches!(runtime.waiting, Waiting::Movie));
        assert!(!runtime.can_save());
        let id = runtime.movie_request().unwrap().id;
        runtime.complete_movie(id + 1, None)?;
        assert!(runtime.movie_request().is_some());
        runtime.complete_movie(id, None)?;
        assert!(runtime.movie_request().is_none());
        assert!(runtime.waiting_key());
        assert_eq!(runtime.report().pc, pc);
        assert_eq!(runtime.scene.dialogue, before);
        runtime.request_menu_movie("avi/lave_op.mpg")?;
        assert_ne!(runtime.movie_request().unwrap().id, id);
        runtime.complete_movie(id, Some("stale"))?;
        assert!(runtime.movie_request().is_some());
        Ok(())
    }
}
