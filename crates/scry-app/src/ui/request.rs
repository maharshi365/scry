//! Feature-local feedback for requests that can publish partial results.

#[derive(Clone, Default)]
pub(super) struct RequestState {
    pub loading: bool,
    pub error: Option<String>,
    pub status: String,
    pub warnings: Vec<String>,
}

#[derive(Clone)]
pub(super) struct RequestSnapshot {
    restart: bool,
    status: String,
    warnings: Vec<String>,
}

impl RequestState {
    pub fn snapshot(&self) -> RequestSnapshot {
        RequestSnapshot {
            restart: self.loading,
            status: self.status.clone(),
            warnings: self.warnings.clone(),
        }
    }

    pub fn restore(&mut self, snapshot: RequestSnapshot) -> bool {
        *self = Self {
            status: snapshot.status,
            warnings: snapshot.warnings,
            ..Self::default()
        };
        snapshot.restart
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StreamOwner {
    Detail,
    Guide,
}

#[derive(Default)]
pub(super) struct StreamRequest {
    pub generation: u64,
    owner: Option<StreamOwner>,
}

impl StreamRequest {
    pub fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.owner = None;
    }

    pub fn start(&mut self, owner: StreamOwner) -> u64 {
        self.invalidate();
        self.owner = Some(owner);
        self.generation
    }

    pub fn accepts(&self, generation: u64, owner: StreamOwner) -> bool {
        self.generation == generation && self.owner == Some(owner)
    }
}
