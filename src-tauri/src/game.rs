use serde::Serialize;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GamePhase {
    Idle,
    Launching,
    Running,
    Finishing,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameStatus {
    pub phase: GamePhase,
    pub revision: u64,
}

pub struct GameState {
    status: Mutex<GameStatus>,
    changed: Box<dyn Fn(GameStatus) + Send + Sync>,
}

impl GameState {
    pub fn new(changed: impl Fn(GameStatus) + Send + Sync + 'static) -> Self {
        Self {
            status: Mutex::new(GameStatus {
                phase: GamePhase::Idle,
                revision: 0,
            }),
            changed: Box::new(changed),
        }
    }

    pub fn snapshot(&self) -> GameStatus {
        *self
            .status
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    pub fn is_busy(&self) -> bool {
        self.snapshot().phase != GamePhase::Idle
    }

    pub fn begin(self: &Arc<Self>) -> Result<GameSession, String> {
        let mut status = self
            .status
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if status.phase != GamePhase::Idle {
            return Err("Игра уже запущена или готовится к запуску".into());
        }
        status.phase = GamePhase::Launching;
        status.revision += 1;
        (self.changed)(*status);
        Ok(GameSession(self.clone()))
    }

    fn set(&self, phase: GamePhase) {
        let mut status = self
            .status
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        status.phase = phase;
        status.revision += 1;
        (self.changed)(*status);
    }
}

pub struct GameSession(Arc<GameState>);
impl GameSession {
    pub fn started(&self) {
        self.0.set(GamePhase::Running);
    }
    pub fn finishing(&self) {
        self.0.set(GamePhase::Finishing);
    }
}
impl Drop for GameSession {
    fn drop(&mut self) {
        self.0.set(GamePhase::Idle);
    }
}

pub fn show_launcher(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub fn restore_hidden_launcher(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if handle
            .get_webview_window("main")
            .is_some_and(|window| !window.is_visible().unwrap_or(true))
        {
            show_launcher(&handle);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lifecycle_rejects_duplicates_until_exit_hooks_finish_and_recovers_from_failed_launch() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let recorded = events.clone();
        let state = Arc::new(GameState::new(move |status| {
            recorded.lock().unwrap().push(status)
        }));
        let session = state.begin().unwrap();
        assert!(state.begin().is_err());
        session.started();
        assert_eq!(state.snapshot().phase, GamePhase::Running);
        assert!(state.begin().is_err());
        session.finishing();
        assert!(state.begin().is_err());
        drop(session);
        assert!(!state.is_busy());
        drop(state.begin().unwrap());
        assert!(!state.is_busy());
        let events = events.lock().unwrap();
        assert!(events
            .windows(2)
            .all(|pair| pair[1].revision > pair[0].revision));
        assert_eq!(events.last().unwrap().phase, GamePhase::Idle);
    }

    #[test]
    fn simultaneous_launches_have_one_owner() {
        let state = Arc::new(GameState::new(|_| {}));
        let barrier = Arc::new(std::sync::Barrier::new(8));
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let state = state.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    let session = state.begin().ok();
                    barrier.wait();
                    session
                })
            })
            .collect();
        let sessions: Vec<_> = threads
            .into_iter()
            .filter_map(|thread| thread.join().unwrap())
            .collect();
        assert_eq!(sessions.len(), 1);
        drop(sessions);
        assert!(!state.is_busy());
    }
}
