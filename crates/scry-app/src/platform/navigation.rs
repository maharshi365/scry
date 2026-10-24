//! Native history input, delivered to the UI on Rune's next wake.
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryDirection {
    Back,
    Forward,
}

#[derive(Clone, Default)]
pub struct NavigationInput(Arc<Mutex<Input>>);

#[derive(Default)]
struct Input {
    pending: VecDeque<HistoryDirection>,
    wake: Option<rune::Waker>,
}

impl NavigationInput {
    pub fn connect(&self, wake: rune::Waker) {
        if let Ok(mut input) = self.0.lock() {
            input.wake = Some(wake);
        }
    }

    #[must_use]
    pub fn drain(&self) -> Vec<HistoryDirection> {
        self.0
            .lock()
            .map(|mut input| input.pending.drain(..).collect())
            .unwrap_or_default()
    }

    #[cfg(windows)]
    pub(in crate::platform) fn push(&self, direction: HistoryDirection) {
        let wake = if let Ok(mut input) = self.0.lock() {
            if input.pending.len() < 32 {
                input.pending.push_back(direction);
            }
            input.wake.clone()
        } else {
            None
        };
        if let Some(wake) = wake {
            wake();
        }
    }

    /// # Errors
    /// Returns an error if native mouse history input cannot be attached.
    pub fn attach(&self, window: &rune::WindowHandle) -> anyhow::Result<()> {
        #[cfg(windows)]
        return super::windows::navigation::attach(window, self.clone());
        #[cfg(not(windows))]
        {
            let _ = window;
            Ok(())
        }
    }
}
