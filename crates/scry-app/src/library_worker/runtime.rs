//! Database thread lifecycle, cancellation, and bounded event delivery.
use super::{Command, Event, operations::apply};
use scry_library::Library;
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread,
    time::Duration,
};

fn emit(
    events: &SyncSender<Event>,
    stop: &AtomicBool,
    wake: &rune::Waker,
    mut event: Event,
) -> bool {
    while !stop.load(Ordering::Acquire) {
        match events.try_send(event) {
            Ok(()) => {
                wake();
                return true;
            }
            Err(TrySendError::Disconnected(_)) => return false,
            Err(TrySendError::Full(pending)) => event = pending,
        }
        // Backpressure must not drop acknowledgments or prevent shutdown.
        thread::park_timeout(Duration::from_millis(25));
    }
    false
}

pub(super) fn run(
    path: &Path,
    commands: &Receiver<Command>,
    events: &SyncSender<Event>,
    stop: &AtomicBool,
    wake: &rune::Waker,
) {
    if stop.load(Ordering::Acquire) {
        return;
    }
    let Ok(mut library) = Library::open(path) else {
        emit(
            events,
            stop,
            wake,
            Event::RequestError("Cannot open the local library".into()),
        );
        return;
    };
    if !emit(events, stop, wake, Event::Ready) {
        return;
    }
    if !achievements(&mut library, events, stop, wake) {
        return;
    }
    while !stop.load(Ordering::Acquire) {
        let command = match commands.recv_timeout(Duration::from_millis(250)) {
            Ok(command) => command,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        if stop.load(Ordering::Acquire) {
            break;
        }
        let mutation = command.acknowledges_mutation();
        let evaluate = command.affects_achievements();
        let event = match apply(&mut library, command) {
            Ok(event) => event,
            Err(_) if mutation => Event::Error("Cannot complete the library request".into()),
            Err(_) => Event::RequestError("Cannot complete the library request".into()),
        };
        let succeeded = !matches!(event, Event::Error(_) | Event::RequestError(_));
        if !emit(events, stop, wake, event) {
            break;
        }
        if succeeded && evaluate && !achievements(&mut library, events, stop, wake) {
            break;
        }
    }
}

fn achievements(
    library: &mut Library,
    events: &SyncSender<Event>,
    stop: &AtomicBool,
    wake: &rune::Waker,
) -> bool {
    let event = match library.evaluate_achievements() {
        Ok(result) => Event::Achievements(result),
        Err(_) => Event::RequestError("Cannot update achievements".into()),
    };
    emit(events, stop, wake, event)
}
